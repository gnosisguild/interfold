// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { createPublicClient, http } from 'viem'
import { localhost, mainnet, sepolia } from 'viem/chains'

import type { Chain, PublicClient } from 'viem'

/**
 * The chain definition for a supported id, or `undefined` when it is not one we know.
 *
 * Unknown is not automatically fatal: with an explicit RPC URL the transport is fully determined,
 * and viem only needs the chain for conveniences like a default endpoint. Refusing outright would
 * mean this SDK could not talk to a network it had every means to reach.
 */
const resolveChain = (chainId: number): Chain | undefined => {
  switch (chainId) {
    case 1:
      return mainnet
    case 11155111:
      return sepolia
    case 31337:
      return localhost
    default:
      return undefined
  }
}

/**
 * Create a public client for reading contracts.
 *
 * Prefer passing `rpcUrl` — typically the CRISP server's own read-only endpoint, via
 * `chainRpcUrl(serverUrl)`. Without one, viem falls back to its default public endpoint for the
 * chain, which is a third-party service this SDK neither controls nor can observe: it is
 * rate-limited per IP, and a caller who has deliberately routed everything else through their own
 * infrastructure would still be depending on it here without knowing.
 *
 * @param chainId - The chain ID of the network
 * @param rpcUrl - Endpoint to read through. Omit only when the default public RPC is acceptable.
 * @returns The public client
 */
export const getPublicClient = (chainId: number, rpcUrl?: string): PublicClient => {
  const chain = resolveChain(chainId)

  if (!chain && !rpcUrl) {
    throw new Error(`Unsupported chainId ${chainId}: pass an rpcUrl to read through`)
  }

  return createPublicClient({
    transport: http(rpcUrl),
    chain,
  })
}
