// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { useCallback, useEffect, useState } from 'react'
import { usePublicClient, useWalletClient } from 'wagmi'
import type { Address } from 'viem'

import { useVoteManagementContext } from '@/context/voteManagement'
import { useNotificationAlertContext } from '@/context/NotificationAlert/NotificationAlert.context.tsx'
import { CensusMode } from '@/model/vote.model'
import { SELF_REGISTRY_ABI, isRegisteredIn } from '@/utils/onchainCensus'
import { getChain } from '@/utils/methods'

/**
 * Registration state for an ONCHAIN round backed by a `SelfRegistry`.
 *
 * Registration is a plain wallet transaction against the round's token — the registry ignores
 * timepoints on purpose, so registering during the input window is exactly what makes the voter
 * eligible for the running round. For rounds that are not ONCHAIN, or whose token is not a
 * registry this client can talk to, everything here stays inert.
 */
export const useRegistration = () => {
  const { user, roundState } = useVoteManagementContext()
  const { showToast } = useNotificationAlertContext()
  const { data: walletClient } = useWalletClient()
  const publicClient = usePublicClient()

  const [isRegistered, setIsRegistered] = useState<boolean | null>(null)
  const [isRegistering, setIsRegistering] = useState<boolean>(false)

  const isOnchainRound = roundState?.census_mode === CensusMode.Onchain
  const registry = isOnchainRound ? (roundState?.token_address as Address | undefined) : undefined

  const userAddress = user?.address

  useEffect(() => {
    let cancelled = false

    const refresh = async () => {
      if (!publicClient || !registry || !userAddress) {
        if (!cancelled) setIsRegistered(null)
        return
      }

      try {
        const registered = await isRegisteredIn(publicClient, registry, userAddress as Address)
        if (!cancelled) setIsRegistered(registered)
      } catch {
        // The round's token is not a registry this client can read — nothing to register into.
        if (!cancelled) setIsRegistered(null)
      }
    }

    refresh()
    return () => {
      cancelled = true
    }
  }, [publicClient, registry, userAddress])

  const register = useCallback(async () => {
    if (!walletClient || !publicClient || !registry) {
      showToast({ type: 'danger', message: 'Wallet not connected' })
      return
    }

    setIsRegistering(true)
    try {
      const hash = await walletClient.writeContract({
        abi: SELF_REGISTRY_ABI,
        address: registry,
        functionName: 'register',
        chain: getChain(),
      })
      const receipt = await publicClient.waitForTransactionReceipt({ hash })

      // The receipt resolves for a reverted transaction too; a revert must not mark the
      // account as registered.
      if (receipt.status !== 'success') {
        throw new Error(`Registration transaction reverted: ${hash}`)
      }

      setIsRegistered(true)
      showToast({ type: 'success', message: 'Registered — you can vote in this round' })
    } catch (error) {
      console.error('Registration failed:', error)
      showToast({ type: 'danger', message: 'Registration failed' })
    } finally {
      setIsRegistering(false)
    }
  }, [walletClient, publicClient, registry, showToast])

  return {
    /** Whether this round lets voters self-register on-chain. */
    canRegister: isOnchainRound && registry !== undefined,
    /** null while unknown or when the round has no registry to ask. */
    isRegistered,
    isRegistering,
    register,
  }
}
