// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { expect } from 'chai'
import { deployCRISPProgram, ethers } from './utils'
import type { CRISPProgram } from '../types'

const CONSTANT = 0
const CUSTOM = 1
const TOKEN = 0
const BY_REQUESTER = 1
const ONCHAIN = 2
/// Mirrors `MAX_VOTE_OPTIONS` in CRISPProgram.sol, which is not public.
const MAX_VOTE_OPTIONS = 10

/// `censusMode` says where a round's electorate comes from, and it is declared rather than inferred.
///
/// A coordinator that probed every requester and fell back on failure would turn a broken census
/// provider into a token vote over the wrong voters — the round would run, and nothing would error.
/// Declaring it also means an impossible combination can be rejected here, in the transaction that
/// requests the E3, rather than by the coordinator minutes later after the fee has been paid.
describe('CRISPProgram census mode', function () {
  this.timeout(120000)

  let crispProgram: CRISPProgram
  let owner: string

  const encode = (creditMode: number, censusMode?: number, numOptions = 2, opts: { token?: string; credits?: number } = {}) => {
    const types = ['address', 'uint256', 'uint256', 'uint256', 'uint256']
    const values: unknown[] = [opts.token ?? ethers.ZeroAddress, 0, numOptions, creditMode, opts.credits ?? 1]
    if (censusMode !== undefined) {
      types.push('uint256')
      values.push(censusMode)
    }
    return ethers.AbiCoder.defaultAbiCoder().encode(types, values)
  }

  const validate = (e3Id: number, params: string) => crispProgram.validate(e3Id, 0, '0x', '0x', params)

  beforeEach(async () => {
    crispProgram = await deployCRISPProgram()
    owner = (await ethers.getSigners())[0].address
    expect(await crispProgram.owner()).to.equal(owner, 'validate is owner-callable in these tests')
  })

  /// Required, not optional. A caller that omits it must fail loudly rather than silently receive
  /// token discovery — which is the same silent-wrong-electorate failure this enum exists to stop,
  /// one level up.
  it('rejects params without a census mode', async () => {
    await expect(validate(1, encode(CONSTANT))).to.be.revert(ethers)
  })

  it('records a declared TOKEN mode', async () => {
    await validate(2, encode(CONSTANT, TOKEN))
    expect(await crispProgram.censusModeOf(2)).to.equal(TOKEN)
  })

  it('records a declared BY_REQUESTER mode', async () => {
    await validate(3, encode(CONSTANT, BY_REQUESTER))
    expect(await crispProgram.censusModeOf(3)).to.equal(BY_REQUESTER)
  })

  /// The pairing that cannot work: a requester-supplied census names who may vote, not how much
  /// each vote weighs. Rejected on chain so it costs nothing rather than failing in the indexer
  /// after the E3 has been paid for.
  it('rejects BY_REQUESTER with custom credits', async () => {
    await expect(validate(4, encode(CUSTOM, BY_REQUESTER))).to.be.revertedWithCustomError(crispProgram, 'CensusModeRequiresConstantCredits')
  })

  it('still allows custom credits with token discovery', async () => {
    await validate(5, encode(CUSTOM, TOKEN))
    expect(await crispProgram.censusModeOf(5)).to.equal(TOKEN)
  })

  /// An unrecognised mode is a coordinator that would not know what to do. Better to refuse the
  /// round than to have it silently treated as a token vote.
  it('rejects an unknown census mode', async () => {
    await expect(validate(6, encode(CONSTANT, 3))).to.be.revertedWithCustomError(crispProgram, 'InvalidCensusMode')
  })

  /// ONCHAIN reads every voter's power from the token, one input at a time. A round that names no
  /// token, or names something that cannot answer `getPastVotes`, accepts no ballot at all — so it
  /// is refused in the request transaction rather than after the fee is paid.
  describe('onchain census', () => {
    it('rejects ONCHAIN without a token', async () => {
      await expect(validate(20, encode(CUSTOM, ONCHAIN))).to.be.revertedWithCustomError(crispProgram, 'CensusModeRequiresToken')
    })

    it('rejects ONCHAIN with an address that holds no code', async () => {
      // An EOA. A call to a codeless address succeeds and returns nothing, so `clock()` fails
      // while decoding rather than inside the call — which `try/catch` does not cover. Without an
      // explicit code check the round is still refused, but by a bare panic rather than a named
      // error, which tells a requester nothing about what to fix.
      const eoa = (await ethers.getSigners())[1].address

      await expect(validate(25, encode(CUSTOM, ONCHAIN, 2, { token: eoa }))).to.be.revertedWithCustomError(
        crispProgram,
        'CensusModeRequiresToken',
      )
    })

    it('rejects ONCHAIN with a token that is not an ERC20Votes', async () => {
      // A plain ERC20. `_previousTimepoint` swallows the missing `clock()` and falls back to block
      // numbers, so without the probe this round would validate and then revert on every input.
      const plain = await ethers.deployContract('MockVotingToken')

      await expect(validate(21, encode(CUSTOM, ONCHAIN, 2, { token: await plain.getAddress() }))).to.be.revertedWithCustomError(
        crispProgram,
        'CensusModeRequiresToken',
      )
    })

    it('rejects ONCHAIN with constant credits of zero', async () => {
      // `credits` becomes the voting-power bound the circuit enforces, so zero accepts only masks.
      const votes = await ethers.deployContract('MockVotesToken')

      await expect(
        validate(22, encode(CONSTANT, ONCHAIN, 2, { token: await votes.getAddress(), credits: 0 })),
      ).to.be.revertedWithCustomError(crispProgram, 'InvalidCredits')
    })

    it('accepts ONCHAIN with a votes token', async () => {
      const votes = await ethers.deployContract('MockVotesToken')

      await validate(23, encode(CUSTOM, ONCHAIN, 2, { token: await votes.getAddress() }))

      expect(await crispProgram.censusModeOf(23)).to.equal(ONCHAIN)
    })

    it('allows constant credits when the allowance is non-zero', async () => {
      const votes = await ethers.deployContract('MockVotesToken')

      await validate(24, encode(CONSTANT, ONCHAIN, 2, { token: await votes.getAddress(), credits: 5 }))

      expect(await crispProgram.censusModeOf(24)).to.equal(ONCHAIN)
    })
  })

  /// The circuit asserts `num_options <= MAX_OPTIONS`, so a round above the cap accepts no ballot:
  /// every vote proof fails. Refuse it at request time rather than storing a round nobody can
  /// vote in. Mirrors `MAX_VOTE_OPTIONS` in CRISPProgram.sol, which is not public.
  it('rejects more options than the circuit allows', async () => {
    await expect(validate(7, encode(CONSTANT, TOKEN, MAX_VOTE_OPTIONS + 1))).to.be.revertedWithCustomError(
      crispProgram,
      'InvalidNumOptions',
    )
  })

  it('accepts a round at exactly MAX_VOTE_OPTIONS options', async () => {
    await validate(8, encode(CONSTANT, TOKEN, MAX_VOTE_OPTIONS))
    expect(await crispProgram.censusModeOf(8)).to.equal(TOKEN)
  })

  it('rejects fewer than two options', async () => {
    await expect(validate(9, encode(CONSTANT, TOKEN, 1))).to.be.revertedWithCustomError(crispProgram, 'InvalidNumOptions')
  })
})
