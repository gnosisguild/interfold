// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { useState, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { useSignTypedData, usePublicClient, useChainId } from 'wagmi'
import { encodeSolidityProof, finishBallotProof, finishMaskProof, prepareBallot } from '@crisp-e3/sdk'
import { ensureCircuits } from '@/utils/circuits'

import { useVoteManagementContext } from '@/context/voteManagement'
import { useNotificationAlertContext } from '@/context/NotificationAlert/NotificationAlert.context.tsx'
import { Poll } from '@/model/poll.model'
import { BroadcastVoteRequest, CensusMode, Vote, VoteStateLite, VotingRound } from '@/model/vote.model'
import { useInterfoldServer } from '../interfold/useInterfoldServer'
import { getRandomVoterToMask } from '@/utils/voters'
import { handleGenericError } from '@/utils/handle-generic-error'
import { NUM_OPTIONS } from '@/utils/constants'
import { ballotTypedData, getBallotDigest, getCrispProgramAddress } from '@/utils/ballotDigest'

const INTERFOLD_API = import.meta.env.VITE_INTERFOLD_API

/// Shared so the guard in `castVoteWithProof` and the one in `handleProofGeneration` cannot drift
/// into telling a voter two different things about the same round.
const ONCHAIN_UNSUPPORTED = 'This round uses an on-chain census, which this client cannot vote in yet.'

/// The end of the slot's chain of usable entries, with the tree index the new input will name as
/// its parent. Not simply the newest entry published: one whose bytes do not reproduce its
/// commitment is never selected by the Secure Process and is never a valid parent, so the server
/// resolves the chain and answers with the entry that actually holds the slot.
const getSlotHead = async (e3Id: string, address: string): Promise<{ ciphertext: Uint8Array; index: number } | undefined> => {
  const response = await fetch(`${INTERFOLD_API}/state/previous-ciphertext`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ round_id: e3Id, address }),
  })

  if (response.status === 404) return undefined
  if (!response.ok) throw new Error(`Failed to fetch previous ciphertext: ${response.statusText}`)

  const body: unknown = await response.json()
  if (
    typeof body !== 'object' ||
    body === null ||
    !('ciphertext' in body) ||
    !Array.isArray(body.ciphertext) ||
    !body.ciphertext.every((value: unknown) => typeof value === 'number' && Number.isInteger(value) && value >= 0 && value <= 255)
  ) {
    throw new Error('Previous ciphertext response contains invalid bytes')
  }

  if (!('index' in body) || typeof body.index !== 'number' || !Number.isInteger(body.index) || body.index < 0) {
    throw new Error('Previous ciphertext response has no usable index')
  }

  return { ciphertext: new Uint8Array(body.ciphertext), index: body.index }
}

export type VotingStep = 'idle' | 'signing' | 'encrypting' | 'generating_proof' | 'broadcasting' | 'confirming' | 'complete' | 'error'

const extractCleanErrorMessage = (errorMessage: string | undefined): string => {
  if (!errorMessage) return 'Failed to broadcast the vote. Please try again.'

  if (errorMessage.includes('Internal error') || errorMessage.includes('-32603')) {
    return 'Transaction failed. The blockchain rejected the vote. Please try again.'
  }
  if (errorMessage.includes('insufficient funds')) {
    return 'Insufficient funds to process the transaction.'
  }
  if (errorMessage.includes('nonce')) {
    return 'Transaction conflict. Please try again.'
  }
  if (errorMessage.includes('gas')) {
    return 'Transaction failed due to gas issues. Please try again.'
  }
  if (errorMessage.includes('reverted')) {
    return 'Transaction was reverted by the contract.'
  }

  if (errorMessage.length > 100) {
    return 'Vote broadcast failed. Please try again.'
  }

  return errorMessage
}

interface VoteData {
  vote: Vote
  slotAddress: string
  balance: bigint
  signature: string
  messageHash: `0x${string}`
  error?: string
}

export const useVoteCasting = (customRoundState?: VoteStateLite | null, customVotingRound?: VotingRound | null) => {
  const {
    user,
    roundState: contextRoundState,
    votingRound: contextVotingRound,
    broadcastVote,
    setTxUrl,
    markVotedInRound,
    hasVotedInCurrentRound,
  } = useVoteManagementContext()

  const roundState = customRoundState ?? contextRoundState
  const votingRound = customVotingRound ?? contextVotingRound

  const { signTypedDataAsync } = useSignTypedData()
  const publicClient = usePublicClient()
  const chainId = useChainId()
  const { getEligibleVoters, getMerkleLeaves } = useInterfoldServer()
  const { showToast } = useNotificationAlertContext()
  const navigate = useNavigate()
  const [isVoting, setIsVoting] = useState<boolean>(false)
  const [isMasking, setIsMasking] = useState<boolean>(false)
  const [votingStep, setVotingStep] = useState<VotingStep>('idle')
  const [lastActiveStep, setLastActiveStep] = useState<VotingStep | null>(null)
  const [stepMessage, setStepMessage] = useState<string>('')

  /**
   * Encrypt the ballot, have the voter sign the digest that binds it, then prove it.
   *
   * The order matters and cannot be rearranged: the digest commits to the ciphertext, so the
   * ballot has to exist before there is anything to sign. That is why signing happens here rather
   * than up front in `handleVote`.
   *
   * A mask follows the same path and carries the same digest — `publishInput` computes one for
   * every input regardless of branch, so a mask that skipped it would be rejected, and one that
   * looked different on chain would defeat the point of masking.
   */
  const handleProofGeneration = useCallback(
    async (vote: Vote, address: string, balance: bigint, isAMask: boolean, merkleLeaves: bigint[]): Promise<string | undefined> => {
      if (!votingRound) throw new Error('No voting round available for proof generation')
      if (!roundState) throw new Error('No round state available for proof generation')
      if (!publicClient) throw new Error('No RPC client available for proof generation')

      // Defence in depth. `castVoteWithProof` refuses these rounds before fetching a census, but
      // this path builds a Merkle witness and must not do so for a round whose ballots are
      // verified by `crisp_onchain` — that proof could never be published.
      if (roundState.census_mode === CensusMode.Onchain) {
        throw new Error(ONCHAIN_UNSUPPORTED)
      }

      try {
        const publicKey = new Uint8Array(votingRound.pk_bytes)
        const head = await getSlotHead(votingRound.round_id, address)
        const e3Id = BigInt(votingRound.round_id)
        const slot = address as `0x${string}`

        // The slot head is passed as a pair or not at all. A ciphertext without its index would be
        // proven against one entry and published against another, so the SDK types the two together
        // and this branches rather than spreading them as separate optional fields.
        const ballot = {
          censusMode: 'merkle',
          vote,
          publicKey,
          balance,
          merkleLeaves,
          slotAddress: address,
          isMaskVote: isAMask,
          numOptions: NUM_OPTIONS,
        } as const

        await ensureCircuits()
        const prepared = await prepareBallot(head ? { ...ballot, previousCiphertext: head.ciphertext, previousIndex: head.index } : ballot)

        const crispProgram = await getCrispProgramAddress(publicClient, roundState.interfold_address as `0x${string}`, e3Id)
        const digest = await getBallotDigest(publicClient, crispProgram, e3Id, slot, prepared.ctCommitment)

        // A mask is not signed. The circuit skips the signature check on that branch, so the
        // placeholder the SDK supplies is enough.
        if (isAMask) {
          return encodeSolidityProof(await finishMaskProof(prepared, digest))
        }

        setVotingStep('signing')
        setLastActiveStep('signing')
        setStepMessage('Please sign the ballot in your wallet...')

        const { domain, types, primaryType } = ballotTypedData(chainId, crispProgram)
        const signature = await signTypedDataAsync({
          domain,
          types,
          primaryType,
          message: { e3Id, slot, ciphertextCommitment: prepared.ctCommitment },
        })

        return encodeSolidityProof(await finishBallotProof(prepared, digest, signature))
      } catch (error) {
        // Logged and rethrown, not shown. `castVoteWithProof` already toasts what it catches, and
        // toasting here as well gave a rejected wallet prompt two notifications.
        const message = error instanceof Error ? error.message : String(error)
        handleGenericError('generateProof', error instanceof Error ? error : new Error(message))
        throw error
      }
    },
    [votingRound, roundState, publicClient, chainId, signTypedDataAsync],
  )

  const resetVotingState = useCallback(() => {
    setVotingStep('idle')
    setLastActiveStep(null)
    setStepMessage('')
    setIsVoting(false)
    setIsMasking(false)
  }, [])

  /**
   * Handles masking a vote by selecting a random eligible voter.
   */
  const handleMask = useCallback(async (): Promise<VoteData> => {
    if (!user || !roundState) {
      throw new Error('Cannot mask vote: Missing user or round state.')
    }

    const eligibleVoters = await getEligibleVoters(roundState.id)

    if (!eligibleVoters || eligibleVoters.length === 0) {
      throw new Error('No eligible voters available for masking')
    }

    try {
      const randomVoterToMask = getRandomVoterToMask(eligibleVoters)

      return {
        vote: [0, 0],
        slotAddress: randomVoterToMask.address,
        balance: BigInt(randomVoterToMask.balance),
        signature: '',
        messageHash: '' as `0x${string}`,
      }
    } catch (error) {
      return {
        vote: [0, 0],
        slotAddress: '',
        balance: 0n,
        signature: '',
        messageHash: '' as `0x${string}`,
        error: (error as Error).message,
      }
    }
  }, [user, roundState, getEligibleVoters])

  /**
   * Handles the voting process including signing the message.
   */
  const handleVote = useCallback(
    async (pollSelected: Poll, slotAddress: string): Promise<VoteData> => {
      if (!roundState) {
        throw new Error('No round state available for voting')
      }

      // No signing here. The ballot digest commits to the ciphertext, so there is nothing to sign
      // until the vote has been encrypted — the wallet prompt now happens inside
      // `handleProofGeneration`.

      // vote is either 0 or 1, so we need to encode the vote accordingly.
      const balance = 1n
      const vote = pollSelected.value === 0 ? [Number(balance), 0] : [0, Number(balance)]

      return {
        signature: '',
        messageHash: '' as `0x${string}`,
        vote,
        slotAddress,
        balance,
      }
    },
    [roundState],
  )

  const castVoteWithProof = useCallback(
    async (pollSelected: Poll | null, isAMask: boolean = false) => {
      if (!isAMask && !pollSelected) {
        console.log('Cannot cast vote: Poll option not selected.')
        showToast({ type: 'danger', message: 'Please select a poll option first.' })
        return
      }
      if (!user || !roundState) {
        console.error('Cannot cast vote: Missing user or round state.')
        showToast({
          type: 'danger',
          message: 'Cannot cast vote. Ensure you are connected, and the round is active.',
          persistent: true,
        })
        return
      }

      try {
        let voteData

        if (isAMask) {
          setIsMasking(true)
          voteData = await handleMask()
        } else {
          setIsVoting(true)
          voteData = await handleVote(pollSelected!, user.address)
        }

        if (voteData.error) {
          throw new Error(voteData.error)
        }

        // Checked before the census is fetched. An ONCHAIN round has no Merkle census, so the
        // empty-leaves error below would fire first and blame the coordinator for a round this
        // client simply cannot vote in.
        if (roundState.census_mode === CensusMode.Onchain) {
          throw new Error(ONCHAIN_UNSUPPORTED)
        }

        // Step 2: Encrypting vote
        setVotingStep('encrypting')
        setLastActiveStep('encrypting')
        setStepMessage('')

        const merkleLeaves = await getMerkleLeaves(roundState.id)

        if (!merkleLeaves || merkleLeaves?.length === 0) {
          throw new Error('No merkle leaves available for proof generation')
        }

        const encodedProof = await handleProofGeneration(
          voteData.vote,
          voteData.slotAddress,
          voteData.balance,
          isAMask,
          merkleLeaves.map((s: string) => BigInt(`0x${s}`)),
        )

        if (!encodedProof) {
          throw new Error('Failed to encrypt vote.')
        }

        // Step 3: Generating proof
        setVotingStep('generating_proof')
        setLastActiveStep('generating_proof')

        // small delay for UX
        await new Promise((resolve) => setTimeout(resolve, 500))

        // Step 4: Broadcasting
        setVotingStep('broadcasting')
        setLastActiveStep('broadcasting')

        const voteRequest: BroadcastVoteRequest = {
          round_id: roundState.id,
          encoded_proof: encodedProof,
          address: voteData.slotAddress,
        }

        const broadcastVoteResponse = await broadcastVote(voteRequest)

        if (broadcastVoteResponse) {
          switch (broadcastVoteResponse.status) {
            case 'success': {
              setVotingStep('complete')
              setStepMessage(`${isAMask ? 'Masking' : 'Vote'} submitted successfully!'`)

              const url = `https://sepolia.etherscan.io/tx/${broadcastVoteResponse.tx_hash}`
              setTxUrl(url)

              if (!isAMask) markVotedInRound(roundState.id)

              const successMessage = isAMask
                ? 'Slot masked successfully'
                : broadcastVoteResponse.is_vote_update
                  ? 'Vote updated successfully!'
                  : 'Vote submitted successfully!'
              showToast({
                type: 'success',
                message: successMessage,
                linkUrl: url,
              })
              navigate(`/result/${roundState.id}/confirmation`)
              break
            }
            case 'failed_broadcast':
              setVotingStep('error')
              showToast({
                type: 'danger',
                message: extractCleanErrorMessage(broadcastVoteResponse.message),
                persistent: true,
              })
              break
            default:
              setVotingStep('error')
              showToast({
                type: 'danger',
                message: extractCleanErrorMessage(broadcastVoteResponse.message),
                persistent: true,
              })
              break
          }
        } else {
          throw new Error('Received no response after broadcasting vote.')
        }
      } catch (error) {
        setVotingStep('error')
        console.error('Vote processing failed:', error)
        showToast({
          type: 'danger',
          message: `Vote failed: ${error instanceof Error ? error.message : String(error)}`,
          persistent: true,
        })
      } finally {
        setIsVoting(false)
        setIsMasking(false)
      }
    },
    [
      user,
      roundState,
      broadcastVote,
      setTxUrl,
      showToast,
      navigate,
      handleProofGeneration,
      markVotedInRound,
      handleMask,
      handleVote,
      getMerkleLeaves,
    ],
  )

  return {
    castVoteWithProof,
    isVoting,
    isMasking,
    votingStep,
    lastActiveStep,
    stepMessage,
    resetVotingState,
    hasVotedInCurrentRound,
  }
}
