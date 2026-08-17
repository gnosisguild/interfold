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
import { mkdirSync, writeFileSync } from 'fs'
import { dirname } from 'path'
import { fileURLToPath } from 'url'
import { deployCRISPProgram, deployHonkVerifier, deployMockInterfold, ethers } from './utils'
import type { CRISPProgram, HonkVerifier, MockInterfold } from '../types'

const keys = generateBFVKeys()
const publicKey = keys.publicKey

/// Where the Rust side reads the same tree from.
/// Regenerate with `UPDATE_INPUT_TREE_FIXTURE=1 pnpm test`, then re-run
/// `cargo test -p e3-compute-provider`.
const FIXTURE = fileURLToPath(new URL('fixtures/input-tree.json', import.meta.url))
const APPEND_FIXTURE = fileURLToPath(new URL('fixtures/input-tree-append.json', import.meta.url))

/// End-to-end over the seam that no single-language test can cover.
///
/// The contract builds each input tree leaf from the published ciphertext bytes and the proven
/// commitment. The Secure Process rebuilds the same tree in Rust and the E3 program compares the
/// two roots, so a one-byte divergence between the implementations makes every round fail with no
/// other symptom.
///
/// This test drives the real path — real BFV ciphertexts, real Noir proofs, real `publishInput` —
/// and records the resulting tree so `input_tree_fixture_matches_the_chain` in
/// `crates/compute-provider` can assert that Rust reproduces the exact on-chain root.
describe('CRISPProgram input tree (e2e)', function () {
  this.timeout(600000)

  let honkVerifier: HonkVerifier
  let mockInterfold: MockInterfold
  let crispProgram: CRISPProgram
  let address: string
  let leaves: bigint[]
  let e3Id: bigint

  const balance = 100n

  /// A ballot bound to this round, with its own ciphertext and commitment.
  async function buildBallot(vote: number[]): Promise<ProofData & { ctCommitment: `0x${string}` }> {
    const [signer] = await ethers.getSigners()
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
    const message = { e3Id, slot: address, ciphertextCommitment: prepared.ctCommitment }
    const ballotSignature = (await signer.signTypedData(domain, types, message)) as `0x${string}`

    const proof = await finishBallotProof(prepared, digest, ballotSignature)
    return { ...proof, ctCommitment: prepared.ctCommitment }
  }

  /// A mask over an existing ciphertext: a zero vote that anyone may append to an occupied slot.
  /// No signature is checked on this path, which is what makes the poisoning case reachable.
  async function buildMaskOver(previousCiphertext: Uint8Array): Promise<ProofData & { ctCommitment: `0x${string}` }> {
    const prepared = await prepareBallot({
      censusMode: 'merkle',
      vote: [0, 0],
      publicKey,
      merkleLeaves: leaves,
      balance,
      slotAddress: address,
      isMaskVote: true,
      numOptions: 2,
      previousCiphertext,
    })
    // `finalCtCommitment`, not `ctCommitment`: for a mask over an existing ballot the contract
    // stores the commitment of the summed ciphertext, and the digest is built over that.
    const digest = (await crispProgram.ballotDigest(e3Id, address, prepared.finalCtCommitment)) as `0x${string}`
    const proof = await finishMaskProof(prepared, digest)
    return { ...proof, ctCommitment: prepared.finalCtCommitment }
  }

  before(async function () {
    mockInterfold = await deployMockInterfold()
    honkVerifier = await deployHonkVerifier()
    crispProgram = await deployCRISPProgram({ mockInterfold, honkVerifier })

    const [signer] = await ethers.getSigners()
    const signature = (await signer.signMessage(SIGNATURE_MESSAGE)) as `0x${string}`
    address = await getAddressFromSignature(signature, SIGNATURE_MESSAGE_HASH)
    leaves = [...[10n, 20n, 30n], hashLeaf(address, balance)]

    e3Id = await mockInterfold.nextE3Id()
    await mockInterfold.request(await crispProgram.getAddress())
  })

  after(() => {
    destroyBBApi()
  })

  it('reproduces the on-chain root in Rust, over real ciphertexts', async function () {
    const ballot = await buildBallot([10, 0])

    await mockInterfold.setCommitteePublicKey(ballot.publicInputs[8])
    await crispProgram.setMerkleRoot(e3Id, generateMerkleTree(leaves).root)

    await crispProgram.publishInput(e3Id, encodeSolidityProof(ballot))

    const [, , , , inputRoot] = await crispProgram.getRoundData(e3Id)
    expect(inputRoot, 'the round must have an input root after publishing').to.not.equal(0n)

    const record = {
      note: 'Generated by tests/input-tree-e2e.test.ts. Asserted by crates/compute-provider.',
      inputRoot: `0x${BigInt(inputRoot).toString(16).padStart(64, '0')}`,
      // publicInputs[7] is the commitment `publishInput` actually stores; recorded alongside the
      // prepared one so a divergence between them is visible rather than silent.
      contractLeaf: (
        await crispProgram.inputLeaf(
          `0x${Buffer.from(ballot.encryptedVote).toString('hex')}`,
          ballot.publicInputs[7],
          address,
        )
      ).toString(),
      inputs: [
        {
          encryptedVote: `0x${Buffer.from(ballot.encryptedVote).toString('hex')}`,
          commitment: ballot.publicInputs[7],
          slot: address,
          preparedCommitment: ballot.ctCommitment,
        },
      ],
    }

    if (process.env.UPDATE_INPUT_TREE_FIXTURE === '1') {
      mkdirSync(dirname(FIXTURE), { recursive: true })
      writeFileSync(FIXTURE, `${JSON.stringify(record, null, 2)}\n`)
      console.log(`[fixture] wrote ${FIXTURE}`)
    }

    // The leaf the contract stored must be the one it computes from the published pair. This is
    // the value the Rust side has to match; the fixture carries it across the language boundary.
    const expectedLeaf = await crispProgram.inputLeaf(
      record.inputs[0].encryptedVote,
      record.inputs[0].commitment,
      record.inputs[0].slot,
    )
    expect(expectedLeaf).to.not.equal(0n)

    // Append-only: one publish, one leaf. `_processVote` never updates in place, so an entry
    // already in the tree cannot be replaced by a later writer.
    const [, , , , , numberOfVotes] = await crispProgram.getRoundData(e3Id)
    expect(numberOfVotes).to.equal(1n)
  })

  /// The premise of the whole fix: the contract cannot tell that the bytes are wrong.
  ///
  /// The proof constrains the commitment, so a submitter can publish any bytes beside it and
  /// `publishInput` still succeeds. Nothing on chain can reject this, which is why the check has
  /// to happen in the Secure Process.
  it('accepts an input whose bytes are not the ciphertext that was proven', async function () {
    // Its own round: a second input from the same slot would be a re-vote, and the ballot here is
    // built as a first vote.
    const forgedE3Id = await mockInterfold.nextE3Id()
    await mockInterfold.request(await crispProgram.getAddress())
    const previous = e3Id
    e3Id = forgedE3Id

    try {
      const ballot = await buildBallot([5, 0])
      await mockInterfold.setCommitteePublicKey(ballot.publicInputs[8])
      await crispProgram.setMerkleRoot(forgedE3Id, generateMerkleTree(leaves).root)

      const [, , , , rootBefore] = await crispProgram.getRoundData(forgedE3Id)

      // A genuine proof, published beside bytes that are not its ciphertext. Nothing on chain can
      // reject this, which is exactly why the Secure Process has to check it.
      const forged = { ...ballot, encryptedVote: new Uint8Array([0xde, 0xad, 0xbe, 0xef]) }
      await crispProgram.publishInput(forgedE3Id, encodeSolidityProof(forged))

      const [, , , , rootAfter] = await crispProgram.getRoundData(forgedE3Id)
      expect(rootAfter, 'the forged input entered the tree').to.not.equal(rootBefore)

      // The leaf reflects the forged bytes, so the Secure Process still reproduces the root while
      // being able to see that this input does not match its commitment.
      const forgedLeaf = await crispProgram.inputLeaf('0xdeadbeef', ballot.publicInputs[7], address)
      const honestLeaf = await crispProgram.inputLeaf(
        `0x${Buffer.from(ballot.encryptedVote).toString('hex')}`,
        ballot.publicInputs[7],
        address,
      )
      expect(forgedLeaf).to.not.equal(honestLeaf)
    } finally {
      e3Id = previous
    }
  })

  /// Append-only, on chain, with a real mask over the ballot already in the slot.
  ///
  /// This is the poisoning case end to end: a genuine mask proof, published beside bytes that are
  /// not the summed ciphertext it proved. The contract cannot reject it. What has to hold is that
  /// the victim's original entry is still a leaf, so the Secure Process can fall back to it.
  it('appends a second entry to a slot instead of replacing the first', async function () {
    const appendE3Id = await mockInterfold.nextE3Id()
    await mockInterfold.request(await crispProgram.getAddress())
    const previous = e3Id
    e3Id = appendE3Id

    try {
      const ballot = await buildBallot([6, 0])
      await mockInterfold.setCommitteePublicKey(ballot.publicInputs[8])
      await crispProgram.setMerkleRoot(appendE3Id, generateMerkleTree(leaves).root)
      await crispProgram.publishInput(appendE3Id, encodeSolidityProof(ballot))

      const [, , , , rootAfterFirst, votesAfterFirst] = await crispProgram.getRoundData(appendE3Id)
      expect(votesAfterFirst).to.equal(1n)
      expect(await crispProgram.getSlotIndex(appendE3Id, address)).to.equal(0n)

      // A third party masks over the slot. No signature is checked on this path.
      const mask = await buildMaskOver(ballot.encryptedVote)
      const poisoned = { ...mask, encryptedVote: new Uint8Array([0xde, 0xad, 0xbe, 0xef]) }
      await crispProgram.publishInput(appendE3Id, encodeSolidityProof(poisoned))

      const [, , , , rootAfterSecond, votesAfterSecond] = await crispProgram.getRoundData(appendE3Id)

      // Append-only: the poisoned entry is a new leaf, and the honest one is untouched at index 0.
      expect(votesAfterSecond, 'the second entry is a new leaf').to.equal(2n)
      expect(rootAfterSecond).to.not.equal(rootAfterFirst)
      expect(await crispProgram.getSlotIndex(appendE3Id, address)).to.equal(1n)

      if (process.env.UPDATE_INPUT_TREE_FIXTURE === '1') {
        const record = {
          note: 'Generated by tests/input-tree-e2e.test.ts. Asserted by crates/compute-provider.',
          inputRoot: `0x${BigInt(rootAfterSecond).toString(16).padStart(64, '0')}`,
          honestIndex: 0,
          inputs: [
            {
              encryptedVote: `0x${Buffer.from(ballot.encryptedVote).toString('hex')}`,
              commitment: ballot.publicInputs[7],
              slot: address,
            },
            { encryptedVote: '0xdeadbeef', commitment: mask.publicInputs[7], slot: address },
          ],
        }
        mkdirSync(dirname(APPEND_FIXTURE), { recursive: true })
        writeFileSync(APPEND_FIXTURE, `${JSON.stringify(record, null, 2)}\n`)
      }
    } finally {
      e3Id = previous
    }
  })
})
