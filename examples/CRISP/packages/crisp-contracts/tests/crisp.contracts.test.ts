// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import {
  hashLeaf,
  generateBFVKeys,
  SIGNATURE_MESSAGE,
  prepareBallot,
  finishBallotProof,
  finishMaskProof,
  getAddressFromSignature,
  encodeSolidityProof,
  generateMerkleTree,
  SIGNATURE_MESSAGE_HASH,
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
  // One round per publishing test. The ballot digest commits to the e3Id, so a ballot built for
  // one round is rejected by another — each publishing test needs its own proof.
  let publishE3Id: bigint
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

    // Rounds created up front so each ballot can be bound to the exact e3Id it is published to.
    // The digest commits to the e3Id, so a ballot built for one round is rejected by another —
    // that rejection is the cross-round replay protection, not a test artefact.
    publishE3Id = await mockInterfold.nextE3Id()
    await mockInterfold.request(await crispProgram.getAddress())

    const domain = {
      name: 'CRISP',
      version: '1',
      chainId: (await ethers.provider.getNetwork()).chainId,
      verifyingContract: await crispProgram.getAddress(),
    }
    const types = {
      Ballot: [
        { name: 'e3Id', type: 'uint256' },
        { name: 'slot', type: 'address' },
        { name: 'ciphertextCommitment', type: 'bytes32' },
      ],
    }

    // Ballots take the digest from the contract itself, so the test exercises the same binding
    // `publishInput` checks rather than a locally rebuilt one.
    const buildVoteProof = async (e3Id: bigint): Promise<ProofData> => {
      const prepared = await prepareBallot({
        censusMode: 'merkle',
        vote,
        publicKey,
        merkleLeaves: leaves,
        balance,
        slotAddress: address,
        isMaskVote: false,
        numOptions: 2,
      })

      const digest = (await crispProgram.ballotDigest(e3Id, address, prepared.ctCommitment)) as `0x${string}`
      const message = { e3Id, slot: address, ciphertextCommitment: prepared.ctCommitment }

      // The contract and ethers must agree on the domain, or every ballot fails for a reason that
      // looks like a bad signature.
      expect(ethers.TypedDataEncoder.hash(domain, types, message)).to.eq(digest)

      // `signTypedData`, not `signMessage`: `ballotDigest` returns an EIP-712 digest, which a
      // wallet signs directly. `signMessage` would add the EIP-191 prefix and sign a different one.
      const ballotSignature = (await signer.signTypedData(domain, types, message)) as `0x${string}`

      return finishBallotProof(prepared, digest, ballotSignature)
    }

    voteProof = await buildVoteProof(publishE3Id)

    const preparedMask = await prepareBallot({
      censusMode: 'merkle',
      vote: [0, 0],
      publicKey,
      merkleLeaves: leaves,
      balance,
      slotAddress: address,
      isMaskVote: true,
      numOptions: 2,
    })
    // A mask carries a real digest and a placeholder signature.
    const maskDigest = (await crispProgram.ballotDigest(publishE3Id, address, preparedMask.ctCommitment)) as `0x${string}`
    maskProof = await finishMaskProof(preparedMask, maskDigest)
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
      const merkleTree = generateMerkleTree(leaves)

      await mockInterfold.setCommitteePublicKey(voteProof.publicInputs[8])

      await crispProgram.setMerkleRoot(publishE3Id, merkleTree.root)

      // Pin every public input the contract reconstructs against the ones the proof was built
      // with. A mismatch here names the field; without it the only symptom is an opaque
      // SumcheckFailed from the verifier, which says nothing about which value diverged.
      const e3 = await mockInterfold.getE3(publishE3Id)
      const [rootOnChain, , numOptionsOnChain] = await crispProgram.getRoundData(publishE3Id)
      const digest = BigInt(await crispProgram.ballotDigest(publishE3Id, address, voteProof.publicInputs[7]))
      const pi = voteProof.publicInputs.map((v) => BigInt(v))

      expect(pi[0], 'prev_ct_commitment').to.eq(0n)
      expect(pi[1], 'digest_hi').to.eq(digest >> 128n)
      expect(pi[2], 'digest_lo').to.eq(digest & ((1n << 128n) - 1n))
      expect(pi[3], 'slot_address').to.eq(BigInt(address))
      expect(pi[4], 'merkle_root').to.eq(BigInt(rootOnChain))
      expect(pi[5], 'is_first_vote').to.eq(1n)
      expect(pi[6], 'num_options').to.eq(BigInt(numOptionsOnChain))
      expect(pi[8], 'committee_public_key').to.eq(BigInt(e3.committeePublicKey))

      await crispProgram.publishInput(publishE3Id, encodeSolidityProof(voteProof))
    })

    /// The regression test for the ballot binding.
    ///
    /// Before the digest became a public input, `hashed_message` was an unconstrained witness: any
    /// signature the slot key had ever produced satisfied the circuit, including one lifted from a
    /// past transaction. The digest now commits to the round, the slot and the ciphertext, so a
    /// ballot is valid for exactly one of each.
    ///
    /// Reverts inside the verifier rather than with `InvalidNoirProof`, because `HonkVerifier.verify`
    /// reverts on a public-input mismatch instead of returning false.
    it('should reject a ballot bound to a different round', async function () {
      const otherE3Id = await mockInterfold.nextE3Id()
      await mockInterfold.request(await crispProgram.getAddress())

      const merkleTree = generateMerkleTree(leaves)
      await mockInterfold.setCommitteePublicKey(voteProof.publicInputs[8])
      await crispProgram.setMerkleRoot(otherE3Id, merkleTree.root)

      // `voteProof` was built for `publishE3Id`. Everything else about it is valid here — same
      // slot, same census, same committee key — so only the round binding rejects it.
      await expect(crispProgram.publishInput(otherE3Id, encodeSolidityProof(voteProof))).to.be.revert(ethers)
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

    /// Reads the round `validate input` published to, rather than publishing a second ballot.
    /// A ballot is bound to one round, so a second round would need a second proof — and proof
    /// generation dominates this suite's runtime, which already times out on CI hardware.
    it('should reflect a published vote in the input tree', async function () {
      const [, , , , inputRoot, numberOfVotes] = await crispProgram.getRoundData(publishE3Id)

      expect(numberOfVotes).to.equal(1n)
      expect(inputRoot).to.not.equal(EMPTY_TREE_ROOT)
    })
  })
})
