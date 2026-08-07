// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { type Vote, MaskVoteProofInputs, ProofInputs, ProofData, VoteProofInputs } from './types'
import { generateMerkleProof, getAddressFromSignature, getZeroVote, getMaxVoteValue, proofToFields } from './utils'
import { generateCircuitInputsImpl } from './circuitInputs'
import { MASK_SIGNATURE, SIGNATURE_MESSAGE_HASH } from './constants'
export { encodeVote, encryptVote, decodeTally, decryptVote, generateBFVKeys } from './encoding'
import { Noir, type CompiledCircuit } from '@noir-lang/noir_js'
import { Barretenberg, BackendType, UltraHonkBackend } from '@aztec/bb.js'
import crispCircuit from '../../../circuits/bin/crisp/target/crisp.json'
import foldCircuit from '../../../circuits/bin/fold/target/crisp_fold.json'
import userDataEncryptionCt0Circuit from '../../../../../circuits/bin/threshold/target/user_data_encryption_ct0.json'
import userDataEncryptionCt1Circuit from '../../../../../circuits/bin/threshold/target/user_data_encryption_ct1.json'
import userDataEncryptionCircuit from '../../../../../circuits/bin/threshold/target/user_data_encryption.json'
import { bytesToHex, encodeAbiParameters, parseAbiParameters, numberToHex, getAddress } from 'viem/utils'
import { Hex } from 'viem'

// Cached Barretenberg API instance — avoids re-initialising WASM + SRS on every proof.
let _bbApi: Barretenberg | null = null
let _bbApiInitPromise: Promise<Barretenberg> | null = null

const getBBApi = async (): Promise<Barretenberg> => {
  if (_bbApi) return _bbApi
  if (_bbApiInitPromise) return _bbApiInitPromise

  _bbApiInitPromise = (async () => {
    try {
      // Outside the browser, bb.js prefers its native Unix-socket backend, allows the bb
      // process only 5s to create the socket, and drops the timeout into an unhandled
      // rejection — callers then wait forever instead of failing. Pin Node to WASM. The
      // browser is unaffected: it already selects the (multi-threaded) worker backend.
      const backend = typeof window === 'undefined' ? { backend: BackendType.Wasm } : {}
      const api = await Barretenberg.new({ srsSize: 2 ** 21, ...backend })
      _bbApi = api
      return api
    } finally {
      _bbApiInitPromise = null
    }
  })()

  return _bbApiInitPromise
}

// Destroy the cached Barretenberg API and free its resources.
export const destroyBBApi = (): void => {
  _bbApiInitPromise = null
  if (_bbApi) {
    _bbApi.destroy()
    _bbApi = null
  }
}

/**
 * Generate the circuit inputs for a vote proof.
 * Runs in a worker when available to avoid blocking the main thread.
 */
export const generateCircuitInputs = async (proofInputs: ProofInputs): Promise<{ circuitInputs: any; encryptedVote: Uint8Array }> => {
  if (typeof Worker !== 'undefined') {
    try {
      const worker = new Worker(new URL('./workers/generateCircuitInputs.worker.js', import.meta.url), { type: 'module' })
      return new Promise((resolve, reject) => {
        worker.onmessage = (
          e: MessageEvent<
            { type: 'result'; circuitInputs: any; encryptedVote: Uint8Array } | { type: 'error'; error: string; stack?: string }
          >,
        ) => {
          worker.terminate()
          if (e.data.type === 'result') {
            resolve({ circuitInputs: e.data.circuitInputs, encryptedVote: e.data.encryptedVote })
          } else {
            reject(new Error(e.data.error))
          }
        }
        worker.onerror = (err) => {
          worker.terminate()
          reject(err)
        }
        worker.postMessage(proofInputs)
      })
    } catch {
      // Worker creation failed (e.g. bundler path resolution); fall back to main thread
    }
  }

  return generateCircuitInputsImpl(proofInputs)
}

/**
 * Execute a circuit.
 * @param circuit - The circuit to execute.
 * @param inputs - The inputs to the circuit.
 * @returns The execute circuit result.
 */
export const executeCircuit = async (circuit: CompiledCircuit, inputs: any): Promise<{ witness: Uint8Array; returnValue: any }> => {
  const noir = new Noir(circuit as CompiledCircuit)

  return noir.execute(inputs)
}

/**
 * Generate a proof for the CRISP circuit given the circuit inputs.
 * @param circuitInputs - Inputs used in both the CRISP and GRECO circuits.
 * @returns The proof.
 */
export const generateProof = async (circuitInputs: any) => {
  const api = await getBBApi()

  const { witness: userDataEncryptionCt0Witness } = await executeCircuit(userDataEncryptionCt0Circuit as CompiledCircuit, {
    pk0is: circuitInputs.pk0is,
    ct0is: circuitInputs.ct0is,
    u: circuitInputs.u,
    e0: circuitInputs.e0,
    e0is: circuitInputs.e0is,
    e0_quotients: circuitInputs.e0_quotients,
    k1: circuitInputs.k1,
    r1is: circuitInputs.r1is,
    r2is: circuitInputs.r2is,
  })
  const { witness: userDataEncryptionCt1Witness } = await executeCircuit(userDataEncryptionCt1Circuit as CompiledCircuit, {
    pk1is: circuitInputs.pk1is,
    ct1is: circuitInputs.ct1is,
    u: circuitInputs.u,
    e1: circuitInputs.e1,
    p1is: circuitInputs.p1is,
    p2is: circuitInputs.p2is,
  })
  const { witness: crispWitness, returnValue: crispReturnValue } = await executeCircuit(crispCircuit as CompiledCircuit, {
    prev_ct0is: circuitInputs.prev_ct0is,
    prev_ct1is: circuitInputs.prev_ct1is,
    prev_ct_commitment: circuitInputs.prev_ct_commitment,
    sum_ct0is: circuitInputs.sum_ct0is,
    sum_ct1is: circuitInputs.sum_ct1is,
    sum_r0is: circuitInputs.sum_r0is,
    sum_r1is: circuitInputs.sum_r1is,
    ct0is: circuitInputs.ct0is,
    ct1is: circuitInputs.ct1is,
    k1: circuitInputs.k1,
    public_key_x: circuitInputs.public_key_x,
    public_key_y: circuitInputs.public_key_y,
    signature: circuitInputs.signature,
    hashed_message: circuitInputs.hashed_message,
    merkle_root: circuitInputs.merkle_root,
    merkle_proof_length: circuitInputs.merkle_proof_length,
    merkle_proof_indices: circuitInputs.merkle_proof_indices,
    merkle_proof_siblings: circuitInputs.merkle_proof_siblings,
    slot_address: circuitInputs.slot_address,
    balance: circuitInputs.balance,
    is_first_vote: circuitInputs.is_first_vote,
    is_mask_vote: circuitInputs.is_mask_vote,
    num_options: circuitInputs.num_options,
  })

  const userDataEncryptionCt0Backend = new UltraHonkBackend((userDataEncryptionCt0Circuit as CompiledCircuit).bytecode, api)
  const userDataEncryptionCt1Backend = new UltraHonkBackend((userDataEncryptionCt1Circuit as CompiledCircuit).bytecode, api)
  const userDataEncryptionBackend = new UltraHonkBackend((userDataEncryptionCircuit as CompiledCircuit).bytecode, api)
  const crispBackend = new UltraHonkBackend((crispCircuit as CompiledCircuit).bytecode, api)
  const foldBackend = new UltraHonkBackend((foldCircuit as CompiledCircuit).bytecode, api)

  const { proof: userDataEncryptionCt0Proof, publicInputs: userDataEncryptionCt0PublicInputs } =
    await userDataEncryptionCt0Backend.generateProof(userDataEncryptionCt0Witness, {
      verifierTarget: 'noir-recursive-no-zk',
    })
  const { proof: userDataEncryptionCt1Proof, publicInputs: userDataEncryptionCt1PublicInputs } =
    await userDataEncryptionCt1Backend.generateProof(userDataEncryptionCt1Witness, {
      verifierTarget: 'noir-recursive-no-zk',
    })
  const { proof: crispProof, publicInputs: crispPublicInputs } = await crispBackend.generateProof(crispWitness, {
    verifierTarget: 'noir-recursive-no-zk',
  })

  const userDataEncryptionCt0Artifacts = await userDataEncryptionCt0Backend.generateRecursiveProofArtifacts(
    userDataEncryptionCt0Proof,
    userDataEncryptionCt0PublicInputs.length,
    {
      verifierTarget: 'noir-recursive-no-zk',
    },
  )
  const userDataEncryptionCt1Artifacts = await userDataEncryptionCt1Backend.generateRecursiveProofArtifacts(
    userDataEncryptionCt1Proof,
    userDataEncryptionCt1PublicInputs.length,
    {
      verifierTarget: 'noir-recursive-no-zk',
    },
  )
  const crispArtifacts = await crispBackend.generateRecursiveProofArtifacts(crispProof, crispPublicInputs.length, {
    verifierTarget: 'noir-recursive-no-zk',
  })

  const { witness: userDataEncryptionWitness } = await executeCircuit(userDataEncryptionCircuit as CompiledCircuit, {
    ct0_verification_key: userDataEncryptionCt0Artifacts.vkAsFields,
    ct0_proof: proofToFields(userDataEncryptionCt0Proof),
    ct0_public_inputs: userDataEncryptionCt0PublicInputs,
    ct0_key_hash: userDataEncryptionCt0Artifacts.vkHash,
    ct1_verification_key: userDataEncryptionCt1Artifacts.vkAsFields,
    ct1_proof: proofToFields(userDataEncryptionCt1Proof),
    ct1_public_inputs: userDataEncryptionCt1PublicInputs,
    ct1_key_hash: userDataEncryptionCt1Artifacts.vkHash,
  })

  const { proof: userDataEncryptionProof, publicInputs: userDataEncryptionPublicInputs } = await userDataEncryptionBackend.generateProof(
    userDataEncryptionWitness,
    {
      verifierTarget: 'noir-recursive-no-zk',
    },
  )
  const userDataEncryptionArtifacts = await userDataEncryptionBackend.generateRecursiveProofArtifacts(
    userDataEncryptionProof,
    userDataEncryptionPublicInputs.length,
    {
      verifierTarget: 'noir-recursive-no-zk',
    },
  )

  const { witness: foldWitness } = await executeCircuit(foldCircuit as CompiledCircuit, {
    user_data_encryption_verification_key: userDataEncryptionArtifacts.vkAsFields,
    user_data_encryption_proof: proofToFields(userDataEncryptionProof),
    user_data_encryption_public_inputs: userDataEncryptionPublicInputs,
    user_data_encryption_key_hash: userDataEncryptionArtifacts.vkHash,
    crisp_verification_key: crispArtifacts.vkAsFields,
    crisp_proof: proofToFields(crispProof),
    crisp_key_hash: crispArtifacts.vkHash,
    prev_ct_commitment: circuitInputs.prev_ct_commitment,
    merkle_root: circuitInputs.merkle_root,
    slot_address: circuitInputs.slot_address,
    is_first_vote: circuitInputs.is_first_vote,
    num_options: circuitInputs.num_options,
    final_ct_commitment: crispReturnValue[0].toString(),
    ct_commitment: crispReturnValue[1].toString(),
    k1_commitment: crispReturnValue[2].toString(),
  })

  const proof = await foldBackend.generateProof(foldWitness, { verifierTarget: 'evm' })

  return proof
}

/**
 * Validate a vote.
 * @param vote - The vote to validate.
 * @param balance - The balance of the voter.
 */
export const validateVote = (vote: Vote, balance: bigint): void => {
  const numChoices = vote.length
  const maxValue = getMaxVoteValue(numChoices)

  for (let i = 0; i < vote.length; i++) {
    if (vote[i] < 0) {
      throw new Error(`Invalid vote: choice ${i} is negative`)
    }
    if (vote[i] > maxValue) {
      throw new Error(`Invalid vote: choice ${i} exceeds maximum encodable value`)
    }
  }

  if (numChoices === 2) {
    // Binary: mutually exclusive
    const nonZeroCount = vote.filter((v) => v > 0).length
    if (nonZeroCount > 1) {
      throw new Error('Invalid vote: for 2 options, only one choice can be non-zero')
    }
    const votedAmount = vote.find((v) => v > 0) ?? 0
    if (votedAmount > balance) {
      throw new Error('Invalid vote: vote exceeds balance')
    }
  } else {
    // 3+ options: split allowed, total capped
    const total = vote.reduce((sum, v) => sum + v, 0)
    if (total > balance) {
      throw new Error(`Invalid vote: total votes (${total}) exceed balance (${balance})`)
    }
  }
}

/**
 * Generate a vote proof for the CRISP circuit given the vote proof inputs.
 * @param voteProofInputs - The vote proof inputs.
 * @returns The vote proof.
 */
export const generateVoteProof = async (voteProofInputs: VoteProofInputs): Promise<ProofData> => {
  // first validate the vote
  validateVote(voteProofInputs.vote, voteProofInputs.balance)

  const address = await getAddressFromSignature(voteProofInputs.signature, voteProofInputs.messageHash)

  const merkleProof = generateMerkleProof(voteProofInputs.balance, address, voteProofInputs.merkleLeaves)

  const { circuitInputs, encryptedVote } = await generateCircuitInputs({
    ...voteProofInputs,
    slotAddress: address,
    merkleProof,
    previousCiphertext: voteProofInputs.previousCiphertext,
    signature: voteProofInputs.signature,
    messageHash: voteProofInputs.messageHash,
    isMaskVote: false,
  })

  return { ...(await generateProof(circuitInputs)), encryptedVote }
}

/**
 * Generate a proof for a vote masking operation.
 * @param maskVoteProofInputs The mask vote proof inputs.
 * @returns
 */
export const generateMaskVoteProof = async (maskVoteProofInputs: MaskVoteProofInputs): Promise<ProofData> => {
  const merkleProof = generateMerkleProof(maskVoteProofInputs.balance, maskVoteProofInputs.slotAddress, maskVoteProofInputs.merkleLeaves)

  const { circuitInputs, encryptedVote } = await generateCircuitInputs({
    ...maskVoteProofInputs,
    signature: MASK_SIGNATURE,
    messageHash: SIGNATURE_MESSAGE_HASH,
    vote: getZeroVote(maskVoteProofInputs.numOptions),
    merkleProof,
    isMaskVote: true,
  })

  return { ...(await generateProof(circuitInputs)), encryptedVote }
}

/**
 * Locally verify a Noir proof.
 * @param proof - The proof to verify.
 * @returns True if the proof is valid, false otherwise.
 */
export const verifyProof = async (proof: ProofData): Promise<boolean> => {
  const api = await getBBApi()
  const foldBackend = new UltraHonkBackend(foldCircuit.bytecode, api)

  return foldBackend.verifyProof(proof, { verifierTarget: 'evm' })
}

/**
 * Encode the proof data into a format that can be used by the CRISP program in Solidity
 * to validate the proof.
 * @param proof The proof data.
 * @returns The encoded proof data as a hex string.
 */
export const encodeSolidityProof = ({ publicInputs, proof, encryptedVote }: ProofData): Hex => {
  const slotAddress = getAddress(numberToHex(BigInt(publicInputs[2]), { size: 20 }))
  const encryptedVoteCommitment = publicInputs[5] as `0x${string}`

  return encodeAbiParameters(parseAbiParameters('bytes, address, bytes32, bytes'), [
    bytesToHex(proof),
    slotAddress,
    encryptedVoteCommitment,
    bytesToHex(encryptedVote),
  ])
}
