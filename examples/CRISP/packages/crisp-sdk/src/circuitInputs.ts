// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { getZkInputsGenerator, encodeVote, encryptVote } from './encoding'
import { extractSignatureComponents, generateMerkleProof, getZeroVote, numberArrayToBigInt64Array } from './utils'
import type { PreparedBallot, PrepareBallotInputs } from './types'

/**
 * Split a 32-byte digest into the two 16-byte halves the circuit takes as public inputs.
 *
 * A Keccak digest is 256 bits and a field element holds fewer than 254, so it cannot cross the
 * circuit boundary in one piece. `crisp_lib::ecdsa::digest_from_halves` rebuilds the 32 bytes and
 * range-checks each half, and `CRISPProgram.publishInput` splits the same way.
 *
 * @param digest The 32-byte ballot digest.
 * @returns The high and low halves as hex field elements.
 */
export const splitDigest = (digest: `0x${string}`): { digestHi: `0x${string}`; digestLo: `0x${string}` } => {
  if (digest.length !== 66) {
    throw new Error(`Invalid digest: expected 32 bytes, got ${(digest.length - 2) / 2}`)
  }

  return {
    digestHi: `0x${digest.slice(2, 34)}`,
    digestLo: `0x${digest.slice(34, 66)}`,
  }
}

/**
 * Phase one of building a ballot: encrypt the vote and build every circuit input that does not
 * depend on the signature.
 *
 * Kept separate from the signature because the digest a voter signs binds the ciphertext, so the
 * ciphertext has to exist first. The returned `ctCommitment` is what `CRISPProgram.ballotDigest`
 * takes as its `ciphertextCommitment` argument.
 *
 * Kept in a separate module so it can run in a worker.
 *
 * @param inputs The ballot to prepare.
 * @returns The partial circuit inputs, the ciphertext, and its commitment.
 */
export const prepareCircuitInputsImpl = async (inputs: PrepareBallotInputs): Promise<PreparedBallot> => {
  const zkInputsGenerator = getZkInputsGenerator()

  const numOptions = inputs.isMaskVote ? inputs.numOptions : inputs.vote.length
  const zeroVote = getZeroVote(numOptions)
  const vote = inputs.isMaskVote ? zeroVote : inputs.vote
  const encodedVote = encodeVote(vote)

  let circuitInputs: any
  let encryptedVote: Uint8Array

  if (!inputs.previousCiphertext) {
    const result = await zkInputsGenerator.generateInputs(
      encryptVote(zeroVote, inputs.publicKey),
      inputs.publicKey,
      numberArrayToBigInt64Array(encodedVote),
    )

    circuitInputs = result.inputs
    encryptedVote = result.encryptedVote
  } else {
    const result = await zkInputsGenerator.generateInputsForUpdate(
      inputs.previousCiphertext,
      inputs.publicKey,
      numberArrayToBigInt64Array(encodedVote),
    )

    circuitInputs = result.inputs
    encryptedVote = result.encryptedVote
  }

  circuitInputs.slot_address = inputs.slotAddress.toLowerCase()
  circuitInputs.is_first_vote = !inputs.previousCiphertext
  circuitInputs.is_mask_vote = inputs.isMaskVote
  circuitInputs.num_options = numOptions.toString()

  if (inputs.censusMode === 'onchain') {
    circuitInputs.voting_power = inputs.votingPower.toString()
  } else {
    // Derived here rather than by the caller. The old API recovered the slot address from the
    // signature to build this, which is no longer possible: the signature now comes after the
    // ciphertext, and the caller states the slot address instead.
    const merkleProof = generateMerkleProof(inputs.balance, inputs.slotAddress, inputs.merkleLeaves)

    circuitInputs.balance = inputs.balance.toString()
    circuitInputs.merkle_root = merkleProof.proof.root.toString()
    circuitInputs.merkle_proof_length = merkleProof.length.toString()
    circuitInputs.merkle_proof_indices = merkleProof.indices.map((i) => i === 1)
    circuitInputs.merkle_proof_siblings = merkleProof.proof.siblings.map((s) => s.toString())
  }

  // Exported by the wasm alongside the witness. Recomputing it here would have to match
  // `compute_ciphertext_commitment` exactly, so it is carried across instead.
  const ctCommitment = `0x${BigInt(circuitInputs.ct_commitment).toString(16).padStart(64, '0')}` as `0x${string}`

  return { circuitInputs, encryptedVote, ctCommitment, censusMode: inputs.censusMode }
}

/**
 * Phase two: attach the signed digest to a prepared ballot.
 *
 * The digest is a public input in both branches, because `CRISPProgram.publishInput` computes it
 * for every input. A mask carries the same digest as a real vote and only skips the signature
 * check inside the circuit, which is what keeps the two indistinguishable on chain.
 *
 * @param prepared The output of `prepareCircuitInputsImpl`.
 * @param digest The digest from `CRISPProgram.ballotDigest`.
 * @param signature The signature over that digest. A mask passes the placeholder signature.
 * @returns The complete circuit inputs.
 */
export const attachSignatureImpl = async (prepared: PreparedBallot, digest: `0x${string}`, signature: `0x${string}`): Promise<any> => {
  const { digestHi, digestLo } = splitDigest(digest)
  const components = await extractSignatureComponents(signature, digest)

  const circuitInputs = prepared.circuitInputs
  circuitInputs.digest_hi = digestHi
  circuitInputs.digest_lo = digestLo
  circuitInputs.public_key_x = Array.from(components.publicKeyX).map((b) => b.toString())
  circuitInputs.public_key_y = Array.from(components.publicKeyY).map((b) => b.toString())
  circuitInputs.signature = Array.from(components.signature).map((b) => b.toString())

  return circuitInputs
}
