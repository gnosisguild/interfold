// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { getZkInputsGenerator, encodeVote, encryptVote } from './encoding'
import { extractSignatureComponents, getZeroVote, numberArrayToBigInt64Array } from './utils'
import type { ProofInputs } from './types'

/**
 * Generate the circuit inputs for a vote proof.
 * Kept in a separate module so it can run in a worker.
 */
export const generateCircuitInputsImpl = async (proofInputs: ProofInputs): Promise<{ circuitInputs: any; encryptedVote: Uint8Array }> => {
  const zkInputsGenerator = getZkInputsGenerator()

  const numOptions = proofInputs.vote.length
  const zeroVote = getZeroVote(numOptions)
  const encodedVote = encodeVote(proofInputs.vote)

  let circuitInputs: any
  let encryptedVote: Uint8Array

  if (!proofInputs.previousCiphertext) {
    const result = await zkInputsGenerator.generateInputs(
      encryptVote(zeroVote, proofInputs.publicKey),
      proofInputs.publicKey,
      numberArrayToBigInt64Array(encodedVote),
    )

    circuitInputs = result.inputs
    encryptedVote = result.encryptedVote
  } else {
    const result = await zkInputsGenerator.generateInputsForUpdate(
      proofInputs.previousCiphertext,
      proofInputs.publicKey,
      numberArrayToBigInt64Array(encodedVote),
    )

    circuitInputs = result.inputs
    encryptedVote = result.encryptedVote
  }

  const signature = await extractSignatureComponents(proofInputs.signature, proofInputs.messageHash)

  circuitInputs.hashed_message = Array.from(signature.messageHash).map((b) => b.toString())
  circuitInputs.public_key_x = Array.from(signature.publicKeyX).map((b) => b.toString())
  circuitInputs.public_key_y = Array.from(signature.publicKeyY).map((b) => b.toString())
  circuitInputs.signature = Array.from(signature.signature).map((b) => b.toString())
  circuitInputs.slot_address = proofInputs.slotAddress.toLowerCase()
  circuitInputs.balance = proofInputs.balance.toString()
  circuitInputs.is_first_vote = !proofInputs.previousCiphertext
  circuitInputs.is_mask_vote = proofInputs.isMaskVote
  circuitInputs.merkle_root = proofInputs.merkleProof.proof.root.toString()
  circuitInputs.merkle_proof_length = proofInputs.merkleProof.length.toString()
  circuitInputs.merkle_proof_indices = proofInputs.merkleProof.indices.map((i) => i === 1)
  circuitInputs.merkle_proof_siblings = proofInputs.merkleProof.proof.siblings.map((s) => s.toString())
  circuitInputs.num_options = numOptions.toString()

  return { circuitInputs, encryptedVote }
}
