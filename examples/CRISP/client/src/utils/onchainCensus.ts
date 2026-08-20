// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { parseAbi } from 'viem'
import type { Address, PublicClient } from 'viem'

const CRISP_PROGRAM_ABI = parseAbi(['function votingPowerOf(uint256 e3Id, address slot) view returns (uint256)'])

/// The `SelfRegistry` surface the client uses: registering, checking registration, and drawing a
/// mask target from the enumerable registrant list. Any votes token can back an ONCHAIN round,
/// but only a registry that answers these is one the client can register into and mask from.
export const SELF_REGISTRY_ABI = parseAbi([
  'function register()',
  'function isRegistered(address account) view returns (bool)',
  'function totalRegistrants() view returns (uint256)',
  'function registrantAt(uint256 index) view returns (address)',
  'function registrants(uint256 start, uint256 count) view returns (address[])',
])

/**
 * The voting power a slot may spend in an ONCHAIN round, in ballot units.
 *
 * Read from the contract rather than recomputed: `publishInput` hands this exact value to the
 * circuit as public input 4, so proving against a locally derived one — re-doing the snapshot,
 * the divisor and the rounding — only surfaces any drift as an opaque verifier failure.
 *
 * @param client The public client.
 * @param crispProgram The CRISP program address.
 * @param e3Id The round.
 * @param slot The slot address.
 * @returns The spendable voting power the proof must be built against.
 */
export const getVotingPower = async (client: PublicClient, crispProgram: Address, e3Id: bigint, slot: Address): Promise<bigint> => {
  return client.readContract({
    address: crispProgram,
    abi: CRISP_PROGRAM_ABI,
    functionName: 'votingPowerOf',
    args: [e3Id, slot],
  })
}

/**
 * Whether an account has registered in a `SelfRegistry`.
 *
 * @param client The public client.
 * @param registry The registry address.
 * @param account The account to check.
 * @returns True when the account is registered.
 */
export const isRegisteredIn = async (client: PublicClient, registry: Address, account: Address): Promise<boolean> => {
  return client.readContract({
    address: registry,
    abi: SELF_REGISTRY_ABI,
    functionName: 'isRegistered',
    args: [account],
  })
}

/**
 * A uniformly random index in `[0, total)`.
 *
 * Rejection sampling over a 256-bit draw. A plain modulo favours low indices whenever `total`
 * does not divide the draw range, and the mask target must be uniform — a skew in who receives
 * cover is a skew in who is deniable. The draw is as wide as the uint256 `total` itself, so no
 * registry size can exceed the range: a narrower draw would make `range % total` degenerate to
 * `range` once `total` outgrew it, leaving a zero limit that rejects every draw.
 *
 * @param total The exclusive upper bound; must be positive.
 * @returns A uniform index below `total`.
 */
const uniformRandomIndex = (total: bigint): bigint => {
  const range = 2n ** 256n
  const limit = range - (range % total)

  let draw: bigint
  do {
    draw = crypto.getRandomValues(new Uint8Array(32)).reduce((acc, byte) => (acc << 8n) | BigInt(byte), 0n)
  } while (draw >= limit)

  return draw % total
}

/**
 * A uniformly random registrant of a `SelfRegistry`, as a mask target.
 *
 * Read live from the registry rather than from the server's holder list, which is discovered
 * once at round start — a registry admits voters during the input window, and someone who
 * registered a minute ago deserves mask cover as much as anyone.
 *
 * @param client The public client.
 * @param registry The registry address.
 * @returns A registrant address, or undefined when nobody has registered yet.
 */
export const getRandomRegistrant = async (client: PublicClient, registry: Address): Promise<Address | undefined> => {
  const total = await client.readContract({
    address: registry,
    abi: SELF_REGISTRY_ABI,
    functionName: 'totalRegistrants',
  })

  if (total === 0n) return undefined

  const index = uniformRandomIndex(total)

  return client.readContract({
    address: registry,
    abi: SELF_REGISTRY_ABI,
    functionName: 'registrantAt',
    args: [index],
  })
}
