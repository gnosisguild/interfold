// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { generateBFVKeys, prepareBallot, finishBallotProof, encodeSolidityProof, destroyBBApi } from '@crisp-e3/sdk'
import type { ProofData } from '@crisp-e3/sdk'
import { expect } from 'chai'
import { deployCRISPProgram, deployHonkVerifier, deployMockInterfold, deployOnchainHonkVerifier, ethers } from './utils'
import type { CRISPProgram, HonkVerifier, MockInterfold } from '../types'

const CONSTANT = 0
const CUSTOM = 1
const ONCHAIN = 2

/// End-to-end coverage for `CensusMode.ONCHAIN`.
///
/// Every other suite substitutes the Merkle verifier for the ONCHAIN one (see `deployCRISPProgram`),
/// because the constructor only needs a non-zero address until a real ONCHAIN ballot is verified.
/// That substitution means nothing here was ever exercised: the `crisp_onchain` circuit, the
/// verifier generated from it, and the path in `publishInput` that reads voting power from the
/// token and hands it to the circuit as public input 4.
///
/// It also means a swapped constructor argument would be invisible — passing the same address
/// twice cannot detect an order mistake. The last test in this file pins that.
describe('CRISP on-chain census', function () {
  // Proof generation dominates; the same budget as the Merkle end-to-end suite.
  this.timeout(600000)

  const keys = generateBFVKeys()
  const publicKey = keys.publicKey

  let mockInterfold: MockInterfold
  let honkVerifier: HonkVerifier
  let onchainHonkVerifier: HonkVerifier
  let crispProgram: CRISPProgram
  let token: any
  let voter: any
  let slotAddress: string
  let e3Id: bigint
  let votingPower: bigint
  let voteProof: ProofData

  const numOptions = 2
  const vote = [7, 0]

  /// Mirrors the tuple `CRISPProgram._initRound` decodes.
  const encodeParams = (opts: {
    token: string
    minVotingPower: bigint
    numOptions: number
    creditMode: number
    credits: bigint
    censusMode: number
  }) =>
    ethers.AbiCoder.defaultAbiCoder().encode(
      ['address', 'uint256', 'uint256', 'uint256', 'uint256', 'uint256'],
      [opts.token, opts.minVotingPower, opts.numOptions, opts.creditMode, opts.credits, opts.censusMode],
    )

  before(async function () {
    mockInterfold = await deployMockInterfold()
    honkVerifier = await deployHonkVerifier()
    onchainHonkVerifier = await deployOnchainHonkVerifier()
    crispProgram = await deployCRISPProgram({ mockInterfold, honkVerifier, onchainHonkVerifier })

    voter = (await ethers.getSigners())[0]
    slotAddress = await voter.getAddress()

    // The snapshot is `clock() - 1`, so the balance has to exist strictly before the round is
    // requested. Minting self-delegates, which ERC20Votes requires for any voting power at all.
    token = await ethers.deployContract('MockVotesToken')
    await token.waitForDeployment()
    await (await token.mint(slotAddress, ethers.parseEther('50'))).wait()
    // Move the clock so the mint lands at a settled timepoint.
    await ethers.provider.send('evm_mine', [])

    e3Id = await mockInterfold.nextE3Id()
    // CUSTOM credits, so the weight the circuit enforces is the token balance itself rather than a
    // flat per-voter allowance. That is what makes this exercise the token read.
    const requestTx = await mockInterfold.requestWithParams(
      await crispProgram.getAddress(),
      numOptions,
      encodeParams({
        token: await token.getAddress(),
        minVotingPower: 1n,
        numOptions,
        creditMode: CUSTOM,
        credits: 1n,
        censusMode: ONCHAIN,
      }),
    )
    const receipt = await requestTx.wait()

    // There is no public getter for the snapshot, so derive it the way `_initRound` does:
    // `_previousTimepoint` is `token.clock() - 1`, and this token's clock is `block.timestamp`.
    const requestBlock = await ethers.provider.getBlock(receipt!.blockNumber)
    const snapshot = BigInt(requestBlock!.timestamp) - 1n

    votingPower = await token.getPastVotes(slotAddress, snapshot)
    expect(votingPower, 'voter must hold power at the snapshot').to.be.greaterThan(0n)

    voteProof = await buildOnchainProof(votingPower)
  })

  after(() => {
    destroyBBApi()
  })

  /// Builds a ballot for the ONCHAIN circuit at a caller-chosen voting power, so a test can prove
  /// a power the token does not agree with.
  async function buildOnchainProof(power: bigint, roundId: bigint = e3Id): Promise<ProofData> {
    const prepared = await prepareBallot({
      censusMode: 'onchain',
      vote,
      publicKey,
      votingPower: power,
      slotAddress,
      isMaskVote: false,
      numOptions,
    })

    const digest = (await crispProgram.ballotDigest(roundId, slotAddress, prepared.ctCommitment)) as `0x${string}`
    const domain = {
      name: 'CRISP',
      version: '1',
      chainId: (await ethers.provider.getNetwork()).chainId,
      verifyingContract: await crispProgram.getAddress(),
    }
    const types = {
      Ballot: [
        { name: 'e3Id', type: 'uint256' },
        { name: 'slot', type: 'address' },
        { name: 'ciphertextCommitment', type: 'bytes32' },
      ],
    }
    const signature = (await voter.signTypedData(domain, types, {
      e3Id: roundId,
      slot: slotAddress,
      ciphertextCommitment: prepared.ctCommitment,
    })) as `0x${string}`

    return finishBallotProof(prepared, digest, signature)
  }

  /// Opens another ONCHAIN round over the same token, so a test can publish a first vote for a
  /// slot that has already voted in `e3Id`.
  async function openRound(): Promise<bigint> {
    const id = await mockInterfold.nextE3Id()
    await (
      await mockInterfold.requestWithParams(
        await crispProgram.getAddress(),
        numOptions,
        encodeParams({
          token: await token.getAddress(),
          minVotingPower: 1n,
          numOptions,
          creditMode: CUSTOM,
          credits: 1n,
          censusMode: ONCHAIN,
        }),
      )
    ).wait()
    return id
  }

  it('verifies an ONCHAIN ballot against the onchain verifier', async function () {
    const isValid = await onchainHonkVerifier.verify(voteProof.proof, voteProof.publicInputs)

    expect(isValid).to.be.true
  })

  /// The two circuits agree on every public input except index 4, so this is the one position that
  /// distinguishes them. Pinning it means a future layout change names the field instead of
  /// surfacing as an opaque verifier revert.
  it('puts the token voting power at public input 4', async function () {
    const pi = voteProof.publicInputs.map((v: string) => BigInt(v))

    expect(pi[3], 'slot_address').to.eq(BigInt(slotAddress))
    expect(pi[4], 'voting_power').to.eq(votingPower)
    expect(pi[5], 'is_first_vote').to.eq(1n)
    expect(pi[6], 'num_options').to.eq(BigInt(numOptions))
  })

  it('publishes an ONCHAIN ballot end to end', async function () {
    await (await mockInterfold.setCommitteePublicKey(voteProof.publicInputs[8])).wait()

    await crispProgram.publishInput(e3Id, encodeSolidityProof(voteProof))
  })

  /// The contract reads the power from the token rather than trusting the ballot. A proof built
  /// for a different power therefore fails, which is what stops a voter inflating their own weight.
  ///
  /// Runs in a fresh round on purpose. Reusing `e3Id` would leave the slot already voted, so the
  /// inflated ballot would mismatch on `prev_ct_commitment` and `is_first_vote` too — it would
  /// still revert, but not for the reason under test.
  it('rejects a ballot proving a voting power the token does not report', async function () {
    const round = await openRound()

    const inflated = await buildOnchainProof(votingPower * 2n, round)
    await (await mockInterfold.setCommitteePublicKey(inflated.publicInputs[8])).wait()
    await expect(crispProgram.publishInput(round, encodeSolidityProof(inflated))).to.be.revert(ethers)

    // Positive control in the same round and the same slot: the honest power publishes. The only
    // difference between the two ballots is the power, so the revert above is attributable to it.
    const honest = await buildOnchainProof(votingPower, round)
    await (await mockInterfold.setCommitteePublicKey(honest.publicInputs[8])).wait()
    await crispProgram.publishInput(round, encodeSolidityProof(honest))
  })

  /// The check the shared-verifier substitution can never make: the two verifiers are not
  /// interchangeable. If the constructor arguments were ever swapped, ONCHAIN ballots would be
  /// checked by the Merkle verifier and this would be the test that noticed.
  it('is rejected by the Merkle verifier', async function () {
    await expect(honkVerifier.verify(voteProof.proof, voteProof.publicInputs)).to.be.revert(ethers)
  })
})
