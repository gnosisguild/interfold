// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { parseAbi } from 'viem'
import type { Address, PublicClient } from 'viem'

const INTERFOLD_ABI = parseAbi([
  'function getE3(uint256 e3Id) view returns ((uint256 seed, uint8 committeeSize, uint256 requestBlock, uint256[2] inputWindow, bytes32 encryptionSchemeId, address e3Program, uint8 paramSet, bytes customParams, address decryptionVerifier, address pkVerifier, bytes32 committeePublicKey, bytes32 ciphertextOutput, bytes plaintextOutput, address requester, bytes32 ciphertextCommitment))',
])

const CRISP_PROGRAM_ABI = parseAbi([
  'function ballotDigest(uint256 e3Id, address slot, bytes32 ciphertextCommitment) view returns (bytes32)',
])

/**
 * Resolve the CRISP program a round was requested against.
 *
 * Read from the round rather than configured, so it cannot disagree with the program the round
 * actually points at. The client already knows the Interfold address from the round state, and a
 * round names its own program, so no extra configuration reaches the browser.
 *
 * @param client The public client.
 * @param interfoldAddress The Interfold contract for this round.
 * @param e3Id The round.
 * @returns The CRISP program address.
 */
export const getCrispProgramAddress = async (client: PublicClient, interfoldAddress: Address, e3Id: bigint): Promise<Address> => {
  const e3 = await client.readContract({
    address: interfoldAddress,
    abi: INTERFOLD_ABI,
    functionName: 'getE3',
    args: [e3Id],
  })

  return e3.e3Program
}

/**
 * Read the digest a voter signs to authorise one ballot.
 *
 * Read from the contract rather than rebuilt here. `CRISPProgram.publishInput` recomputes this
 * digest and the circuit proves the signature covers it, so a locally built EIP-712 struct that
 * drifted from the contract would produce ballots that every node rejects.
 *
 * @param client The public client.
 * @param crispProgram The CRISP program address.
 * @param e3Id The round the ballot belongs to.
 * @param slot The slot address the ballot is written to.
 * @param ciphertextCommitment The commitment from `prepareBallot`.
 * @returns The digest to sign.
 */
export const getBallotDigest = async (
  client: PublicClient,
  crispProgram: Address,
  e3Id: bigint,
  slot: Address,
  ciphertextCommitment: `0x${string}`,
): Promise<`0x${string}`> => {
  return client.readContract({
    address: crispProgram,
    abi: CRISP_PROGRAM_ABI,
    functionName: 'ballotDigest',
    args: [e3Id, slot, ciphertextCommitment],
  })
}

/**
 * The EIP-712 domain and type a ballot signature covers.
 *
 * Must match `CRISPProgram`'s `EIP712("CRISP", "1")` and `BALLOT_TYPEHASH`. A wallet signs this
 * through `signTypedData`, which produces a signature over the same digest `ballotDigest`
 * returns — `signMessage` would add the EIP-191 prefix and sign something else.
 *
 * @param chainId The chain the program is deployed on.
 * @param crispProgram The CRISP program address.
 * @returns The domain and types for `signTypedData`.
 */
export const ballotTypedData = (chainId: number, crispProgram: Address) =>
  ({
    domain: { name: 'CRISP', version: '1', chainId, verifyingContract: crispProgram },
    types: {
      Ballot: [
        { name: 'e3Id', type: 'uint256' },
        { name: 'slot', type: 'address' },
        { name: 'ciphertextCommitment', type: 'bytes32' },
      ],
    },
    primaryType: 'Ballot',
  }) as const
