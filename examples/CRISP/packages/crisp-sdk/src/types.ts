// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import type { LeanIMTMerkleProof } from '@zk-kit/lean-imt'

/**
 * Type representing the details of a specific round in a more convenient format
 * (camelCase view of the server's `state/lite` response)
 */
export type RoundDetails = {
  e3Id: bigint
  chainId: bigint
  interfoldAddress: string
  status: string
  voteCount: bigint
  startTime: bigint
  endTime: bigint
  /// The block the E3 was requested at
  startBlock: bigint
  /// The block the census was built at
  snapshotBlock: bigint
  committeePublicKey: Uint8Array
  emojis: [string, string]
  tokenAddress: string
  balanceThreshold: bigint
  numOptions: bigint
  requester: string
  creditMode: CreditMode
  credits?: bigint
}

/**
 * Type representing the round data stored in the CRISPProgram contract
 */
export type OnChainRoundData = {
  /// The merkle root of the census
  merkleRoot: bigint
  /// The hash of the E3 program params
  paramsHash: `0x${string}`
  /// The number of vote options
  numOptions: bigint
  /// The credit mode of the round
  creditMode: CreditMode
  /// The root of the merkle tree holding the encrypted votes
  inputRoot: bigint
  /// The number of votes published on chain
  numberOfVotes: bigint
}

/**
 * Type representing the token details required for participation in a round
 */
export type TokenDetails = {
  tokenAddress: string
  threshold: bigint
  snapshotBlock: bigint
}

/**
 * Type representing a Merkle proof
 */
export type MerkleProof = {
  leaf: bigint
  index: number
  proof: LeanIMTMerkleProof<bigint>
  length: number
  indices: number[]
}

/**
 * Type representing a vote
 */
export type Vote = number[]

/**
 * Type representing a decoded tally: one total per option.
 *
 * @remarks
 * Uses `bigint` because an aggregated tally coefficient is a sum over all ballots,
 * so a total can exceed `Number.MAX_SAFE_INTEGER`. This matches the `BigUint` the
 * CRISP server returns and the `uint256` the CRISP contract returns.
 */
export type TallyResult = bigint[]

/**
 * Type representing a vector with coefficients
 */
export type Polynomial = {
  coefficients: string[]
}

/**
 * Type representing cryptographic parameters
 */
export type GrecoCryptographicParams = {
  q_mod_t: string
  qis: string[]
  k0is: string[]
}

/**
 * Type representing Greco bounds
 */
export type GrecoBoundParams = {
  e_bound: string
  u_bound: string
  k1_low_bound: string
  k1_up_bound: string
  p1_bounds: string[]
  p2_bounds: string[]
  pk_bounds: string[]
  r1_low_bounds: string[]
  r1_up_bounds: string[]
  r2_bounds: string[]
}

/**
 * Type representing Greco parameters
 */
export type GrecoParams = {
  crypto: GrecoCryptographicParams
  bounds: GrecoBoundParams
}

export type ProofData = {
  publicInputs: string[]
  proof: Uint8Array
  encryptedVote: Uint8Array
}

/**
 * Which circuit a ballot is built for.
 *
 * `merkle` proves membership of a census tree, and matches `CensusMode.TOKEN` and
 * `CensusMode.BY_REQUESTER`. `onchain` takes voting power as a public input that the contract
 * reads from the token, and matches `CensusMode.ONCHAIN`.
 */
export type CensusVariant = 'merkle' | 'onchain'

type PrepareBallotInputsBase = {
  previousCiphertext?: Uint8Array
  publicKey: Uint8Array
  slotAddress: string
  isMaskVote: boolean
  /// Read for a mask, where there is no vote to take a length from.
  numOptions: number
  vote: Vote
}

/**
 * Everything needed to encrypt a ballot, before the voter has signed anything.
 */
export type PrepareBallotInputs =
  | (PrepareBallotInputsBase & { censusMode: 'merkle'; balance: bigint; merkleLeaves: string[] | bigint[] })
  | (PrepareBallotInputsBase & { censusMode: 'onchain'; votingPower: bigint })

/**
 * A ballot that is encrypted but not yet signed.
 *
 * `ctCommitment` is the value to pass to `CRISPProgram.ballotDigest`. Reading the digest from the
 * contract rather than rebuilding the EIP-712 struct here keeps one implementation of the domain.
 */
export type PreparedBallot = {
  circuitInputs: any
  encryptedVote: Uint8Array
  ctCommitment: `0x${string}`
  censusMode: CensusVariant
}

/**
 * `Omit` that maps over each member of a union instead of collapsing it.
 *
 * A plain `Omit` on a discriminated union merges the members and loses the discriminant, which
 * would let a caller pass `censusMode: 'onchain'` with no `votingPower`.
 */
type DistributiveOmit<T, K extends keyof never> = T extends unknown ? Omit<T, K> : never

/**
 * A {@link PrepareBallotInputs} plus the round it belongs to.
 *
 * The SDK resolves `previousCiphertext` from the server, so callers do not pass it.
 */
export type PrepareBallotRequest = { e3Id: bigint } & DistributiveOmit<PrepareBallotInputs, 'previousCiphertext'>

/**
 * Type representing the current round returned by the CRISP server (`rounds/current`)
 */
export type CurrentRoundResponse = {
  id: string
}

/**
 * Type representing the lite state of a round returned by the CRISP server (`state/lite`)
 */
export type E3StateLiteResponse = {
  id: string
  chain_id: number
  interfold_address: string
  status: string
  vote_count: number
  start_time: number
  end_time: number
  start_block: number
  snapshot_block: number
  committee_public_key: number[]
  emojis: [string, string]
  token_address: string
  balance_threshold: string
  num_options: string
  requester: string
  credit_mode: CreditMode
  credits: string | null
}

/**
 * Type representing a generic message response from the CRISP server
 */
export type JsonResponse = {
  response: string
}

/**
 * Type representing a request to start a new E3 round (`rounds/request`)
 */
export type NewRoundRequest = {
  cronApiKey: string
  tokenAddress: string
  balanceThreshold: string
}

/**
 * Type representing a request to broadcast an encrypted vote (`voting/broadcast`)
 */
export type BroadcastVoteRequest = {
  e3Id: bigint
  encodedProof: string
  address: string
}

/**
 * The status of a vote broadcast returned by the CRISP server
 */
export type VoteResponseStatus = 'success' | 'user_already_voted' | 'failed_broadcast'

/**
 * Type representing the response to a vote broadcast (`voting/broadcast`)
 */
export type BroadcastVoteResponse = {
  status: VoteResponseStatus
  tx_hash: string | null
  message: string | null
  is_vote_update?: boolean
}

/**
 * Type representing the vote status of an address in a round (`voting/status`)
 */
export type VoteStatusResponse = {
  round_id: string
  address: string
  has_voted: boolean
  round_status: string | null
}

/**
 * Type representing the result of a round (`state/result` and `state/all`)
 */
export type WebResultResponse = {
  round_id: string
  tally: string[]
  option_1_emoji: string
  option_2_emoji: string
  total_votes: number
  end_time: number
  requester: string
}

/**
 * Type representing a token holder with their address and balance (`state/eligible-addresses`)
 */
export type TokenHolder = {
  address: string
  balance: string
}

/**
 * Enum representing the credit mode for a round, which can be either constant or custom.
 * In constant mode, all voters receive the same amount of credits, while in custom mode,
 * the credits can vary based on certain criteria (e.g., voter balance).
 */
export enum CreditMode {
  CONSTANT = 0,
  CUSTOM = 1,
}
