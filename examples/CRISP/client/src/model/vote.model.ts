// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { CreditMode } from '@crisp-e3/sdk'

/**
 * Where a round's electorate comes from. Mirrors `CensusMode` in the CRISP server and
 * `CRISPProgram`, including the discriminants — they cross the wire as numbers.
 */
export enum CensusMode {
  Token = 0,
  ByRequester = 1,
  Onchain = 2,
}

export interface VotingRound {
  round_id: string
  pk_bytes: number[]
}

export interface CurrentRound {
  id: string
}

export interface BroadcastVoteRequest {
  round_id: string
  encoded_proof: string
  address: string
}

export type VoteResponseStatus = 'success' | 'failed_broadcast'
export interface BroadcastVoteResponse {
  status: VoteResponseStatus
  tx_hash?: string
  message?: string
  is_vote_update?: boolean
}

export interface VoteStatusRequest {
  round_id: string
  address: string
}

export interface VoteStatusResponse {
  round_id: string
  address: string
  has_voted: boolean
  round_status?: string
}

export interface VoteStateLite {
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

  credit_mode: CreditMode
  census_mode: CensusMode
  credits?: number
}

export type Vote = number[]

export interface EligibleVoter {
  address: string
  balance: number
}
