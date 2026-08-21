// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { EligibleVoter } from '@/model/vote.model'

/**
 * Get a random voter details from a list of eligible voters
 * @param addresses The list of eligible voters
 * @returns The randomly selected voter details
 */
export const getRandomVoterToMask = (voters: EligibleVoter[]): EligibleVoter => {
  if (voters.length === 0) {
    throw new Error('No eligible voters available to select from.')
  }

  // Rejection sampling: a plain modulo of a 32-bit draw favours low indices whenever the list
  // length is not a power of two, and the mask target must be uniform.
  const range = 2 ** 32
  const limit = range - (range % voters.length)
  let draw: number
  do {
    draw = crypto.getRandomValues(new Uint32Array(1))[0]
  } while (draw >= limit)

  return voters[draw % voters.length]
}
