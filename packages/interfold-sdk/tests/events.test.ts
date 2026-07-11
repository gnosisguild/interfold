// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { CiphernodeRegistryOwnable__factory } from '@interfold/contracts/types'
import { describe, expect, it } from 'vitest'

import { RegistryEventType } from '../src/events/types'

describe('RegistryEventType', () => {
  it('uses the finalized-committee event name exposed by the registry ABI', () => {
    const registryEventNames = CiphernodeRegistryOwnable__factory.abi.filter((item) => item.type === 'event').map((item) => item.name)

    expect(RegistryEventType.COMMITTEE_FINALIZED).toBe('SortitionCommitteeFinalized')
    expect(registryEventNames).toContain(RegistryEventType.COMMITTEE_FINALIZED)
  })
})
