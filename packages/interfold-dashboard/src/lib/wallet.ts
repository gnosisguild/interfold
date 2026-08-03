// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
// Minimal EIP-1193 wallet connection. The dashboard deliberately avoids a
// connector library (wagmi/rainbowkit): the operator guide only needs one
// injected provider, one chain, and a handful of writes.

import { createWalletClient, custom, type Address, type Hash, type WalletClient } from 'viem'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { CHAIN, publicClient } from './chain'

type Eip1193Provider = {
  request: (args: { method: string; params?: unknown[] | object }) => Promise<any>
  on?: (event: string, handler: (...args: any[]) => void) => void
  removeListener?: (event: string, handler: (...args: any[]) => void) => void
}

export function getProvider(): Eip1193Provider | null {
  const injected = (globalThis as any).window?.ethereum
  return injected ?? null
}

const toChainId = (value: unknown): number | null => {
  if (typeof value === 'number') return value
  if (typeof value === 'string') return Number.parseInt(value, 16)
  return null
}

export type Wallet = {
  available: boolean
  address: Address | null
  chainId: number | null
  onCorrectChain: boolean
  connecting: boolean
  error: string | null
  connect: () => Promise<void>
  switchChain: () => Promise<void>
  /** Wallet client bound to the connected account, or null when disconnected. */
  client: WalletClient | null
}

export function useWallet(): Wallet {
  const provider = useMemo(() => getProvider(), [])
  const [address, setAddress] = useState<Address | null>(null)
  const [chainId, setChainId] = useState<number | null>(null)
  const [connecting, setConnecting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Pick up an already-authorized account on mount without prompting.
  useEffect(() => {
    if (!provider) return undefined
    let cancelled = false
    const load = async () => {
      try {
        const [accounts, hexChain] = await Promise.all([
          provider.request({ method: 'eth_accounts' }),
          provider.request({ method: 'eth_chainId' }),
        ])
        if (cancelled) return
        setAddress((accounts as Address[])[0] ?? null)
        setChainId(toChainId(hexChain))
      } catch {
        // A provider that refuses eth_accounts is simply treated as disconnected.
      }
    }
    void load()

    const onAccounts = (accounts: string[]) => setAddress((accounts[0] as Address) ?? null)
    const onChain = (hexChain: string) => setChainId(toChainId(hexChain))
    provider.on?.('accountsChanged', onAccounts)
    provider.on?.('chainChanged', onChain)
    return () => {
      cancelled = true
      provider.removeListener?.('accountsChanged', onAccounts)
      provider.removeListener?.('chainChanged', onChain)
    }
  }, [provider])

  const connect = useCallback(async () => {
    if (!provider) {
      setError('No Ethereum wallet detected. Install a browser wallet to continue.')
      return
    }
    setConnecting(true)
    setError(null)
    try {
      const accounts = (await provider.request({ method: 'eth_requestAccounts' })) as Address[]
      setAddress(accounts[0] ?? null)
      setChainId(toChainId(await provider.request({ method: 'eth_chainId' })))
    } catch (e) {
      setError(walletErrorMessage(e))
    } finally {
      setConnecting(false)
    }
  }, [provider])

  const switchChain = useCallback(async () => {
    if (!provider) return
    setError(null)
    const hexId = `0x${CHAIN.id.toString(16)}`
    try {
      await provider.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: hexId }] })
      setChainId(CHAIN.id)
    } catch (e: any) {
      // 4902 = chain unknown to the wallet; offer to add it before switching.
      if (e?.code === 4902) {
        try {
          await provider.request({
            method: 'wallet_addEthereumChain',
            params: [
              {
                chainId: hexId,
                chainName: CHAIN.name,
                nativeCurrency: CHAIN.nativeCurrency,
                rpcUrls: [CHAIN.rpcUrls.default.http[0]],
                blockExplorerUrls: [CHAIN.blockExplorers?.default.url],
              },
            ],
          })
          setChainId(CHAIN.id)
          return
        } catch (addError) {
          setError(walletErrorMessage(addError))
          return
        }
      }
      setError(walletErrorMessage(e))
    }
  }, [provider])

  const client = useMemo(() => {
    if (!provider || !address) return null
    return createWalletClient({ account: address, chain: CHAIN, transport: custom(provider) })
  }, [provider, address])

  return {
    available: provider !== null,
    address,
    chainId,
    onCorrectChain: chainId === CHAIN.id,
    connecting,
    error,
    connect,
    switchChain,
    client,
  }
}

// Wallet and node errors are deeply nested and noisy. Surface the shortest
// message a human can act on, and keep custom-error names (the bonding registry
// reverts with typed errors such as `NotBondOwner`).
export function walletErrorMessage(e: unknown): string {
  const err = e as any
  if (err?.code === 4001 || /user rejected|denied transaction/i.test(String(err?.message ?? ''))) {
    return 'Transaction rejected in the wallet.'
  }
  const revertName = err?.cause?.data?.errorName ?? err?.data?.errorName
  if (typeof revertName === 'string') return `Reverted: ${revertName}`
  const short = err?.shortMessage ?? err?.details ?? err?.message
  if (typeof short === 'string' && short.trim() !== '') return short.split('\n')[0]
  return 'Something went wrong. Check the wallet and try again.'
}

/** Wait for a submitted transaction and reject when it reverted on-chain. */
export async function confirmTx(hash: Hash): Promise<void> {
  const receipt = await publicClient.waitForTransactionReceipt({ hash })
  if (receipt.status !== 'success') throw new Error('Transaction reverted on-chain.')
}
