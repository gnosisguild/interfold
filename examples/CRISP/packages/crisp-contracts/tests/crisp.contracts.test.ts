// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import {
  hashLeaf,
  generateBFVKeys,
  SIGNATURE_MESSAGE,
  generateVoteProof,
  getAddressFromSignature,
  encodeSolidityProof,
  generateMerkleTree,
  SIGNATURE_MESSAGE_HASH,
  generateMaskVoteProof,
  destroyBBApi,
} from '@crisp-e3/sdk'
import type { ProofData } from '@crisp-e3/sdk'
import { expect } from 'chai'
import { deployCRISPProgram, deployHonkVerifier, deployMockInterfold, ethers } from './utils'
import type { CRISPProgram, HonkVerifier, MockInterfold } from '../types'

let keys = generateBFVKeys()
let publicKey = keys.publicKey

describe('CRISP Contracts', function () {
  // Allow time for contract deployments + proof generation in before()
  this.timeout(600000)

  let honkVerifier: HonkVerifier
  let mockInterfold: MockInterfold
  let crispProgram: CRISPProgram
  let signature: `0x${string}`
  let address: string
  let leaves: bigint[]
  let voteProof: ProofData
  let maskProof: ProofData
  const balance = 100n
  const vote = [10, 0]

  before(async function () {
    // Deploy contracts once
    mockInterfold = await deployMockInterfold()
    honkVerifier = await deployHonkVerifier()
    crispProgram = await deployCRISPProgram({ mockInterfold, honkVerifier })

    // Compute signature, address, and leaves once
    const [signer] = await ethers.getSigners()
    signature = (await signer.signMessage(SIGNATURE_MESSAGE)) as `0x${string}`
    address = await getAddressFromSignature(signature, SIGNATURE_MESSAGE_HASH)
    leaves = [...[10n, 20n, 30n], hashLeaf(address, balance)]

    // Generate proofs once
    voteProof = await generateVoteProof({
      vote,
      publicKey,
      signature,
      merkleLeaves: leaves,
      balance,
      messageHash: SIGNATURE_MESSAGE_HASH,
      slotAddress: address,
    })

    maskProof = await generateMaskVoteProof({
      publicKey,
      merkleLeaves: leaves,
      balance,
      slotAddress: address,
      numOptions: 2,
    })
  })

  after(() => {
    destroyBBApi()
  })

  // Tally decoding is covered by `tally.decoding.test.ts`, which builds the plaintext
  // output with the real SDK encoder rather than a hand-written fixture.

  describe('validate input', () => {
    it('should verify the proof correctly with the crisp verifier', async function () {
      const verifyGas = await honkVerifier.verify.estimateGas(voteProof.proof, voteProof.publicInputs)
      console.log(`[bench-gas] crisp_user_verify=${verifyGas.toString()}`)
      const isValid = await honkVerifier.verify(voteProof.proof, voteProof.publicInputs)

      expect(isValid).to.be.true
    })

    it('should verify the proof for a vote mask', async function () {
      const isValid = await honkVerifier.verify(maskProof.proof, maskProof.publicInputs)

      expect(isValid).to.be.true
    })

    it('should validate input correctly', async function () {
      const e3Id = await mockInterfold.nextE3Id()
      await mockInterfold.request(await crispProgram.getAddress())

      const merkleTree = generateMerkleTree(leaves)

      await mockInterfold.setCommitteePublicKey(voteProof.publicInputs[6])

      const encodedProof = encodeSolidityProof(voteProof)

      await crispProgram.setMerkleRoot(e3Id, merkleTree.root)

      await crispProgram.publishInput(e3Id, encodedProof)
    })
  })

  describe('get round data', () => {
    // The dynamic input tree has a minimum depth of one (InternalLazyIMT.Z_1).
    const EMPTY_TREE_ROOT = 14744269619966411208579211824598458697587494354926760081771325075741142829156n
    // MockInterfold calls validate with empty e3ProgramParams
    const EMPTY_PARAMS_HASH = ethers.keccak256('0x')

    it('should return empty data for an e3 which was not initialized', async () => {
      const e3Id = await mockInterfold.nextE3Id()

      const [merkleRoot, paramsHash, numOptions, creditMode, inputRoot, numberOfVotes] = await crispProgram.getRoundData(e3Id)

      expect(merkleRoot).to.equal(0n)
      expect(paramsHash).to.equal(ethers.ZeroHash)
      expect(numOptions).to.equal(0n)
      expect(creditMode).to.equal(0n)
      expect(inputRoot).to.equal(EMPTY_TREE_ROOT)
      expect(numberOfVotes).to.equal(0n)
    })

    it('should return the data set by validate', async () => {
      const e3Id = await mockInterfold.nextE3Id()
      await mockInterfold.request(await crispProgram.getAddress())

      const [merkleRoot, paramsHash, numOptions, creditMode, inputRoot, numberOfVotes] = await crispProgram.getRoundData(e3Id)

      expect(merkleRoot).to.equal(0n)
      expect(paramsHash).to.equal(EMPTY_PARAMS_HASH)
      expect(numOptions).to.equal(2n)
      // CreditMode.CONSTANT
      expect(creditMode).to.equal(0n)
      expect(inputRoot).to.equal(EMPTY_TREE_ROOT)
      expect(numberOfVotes).to.equal(0n)
    })

    it('should return the merkle root of the census once set', async () => {
      const e3Id = await mockInterfold.nextE3Id()
      await mockInterfold.request(await crispProgram.getAddress())

      const merkleTree = generateMerkleTree(leaves)
      await crispProgram.setMerkleRoot(e3Id, merkleTree.root)

      const [merkleRoot] = await crispProgram.getRoundData(e3Id)

      expect(merkleRoot).to.equal(BigInt(merkleTree.root))
    })

    it('should reflect a published vote in the input tree', async function () {
      const e3Id = await mockInterfold.nextE3Id()
      await mockInterfold.request(await crispProgram.getAddress())

      const merkleTree = generateMerkleTree(leaves)
      await mockInterfold.setCommitteePublicKey(voteProof.publicInputs[6])
      await crispProgram.setMerkleRoot(e3Id, merkleTree.root)
      await crispProgram.publishInput(e3Id, encodeSolidityProof(voteProof))

      const [, , , , inputRoot, numberOfVotes] = await crispProgram.getRoundData(e3Id)

      expect(numberOfVotes).to.equal(1n)
      expect(inputRoot).to.not.equal(EMPTY_TREE_ROOT)
    })
  })
})
