// SPDX-License-Identifier: LGPL-3.0-only

const SUPPORTED_VRF_CHAIN_IDS = new Set([1, 1_337, 31_337, 11_155_111]);
const ETHEREUM_VRF_CHAIN_IDS = new Set([1, 11_155_111]);

const ETHEREUM_SLOT_SECONDS = 12n;
const VRF_FULFILLMENT_BUFFER_SECONDS = 15n * 60n;

export function assertSupportedVrfChain(chainId: number): void {
  if (!SUPPORTED_VRF_CHAIN_IDS.has(chainId)) {
    throw new Error(
      `VRF sortition supports Ethereum mainnet, Sepolia, and local development chains only; received chainId ${chainId}`,
    );
  }
}

/**
 * Return the minimum configured response window for the selected confirmation count.
 *
 * Ethereum uses 12-second slots. The extra 15 minutes covers oracle submission and short chain or
 * RPC delays. This check rejects configurations that normally expire before fulfillment. It does
 * not guarantee Chainlink liveness.
 */
export function minimumVrfRequestTimeout(
  chainId: number,
  requestConfirmations: number,
): bigint {
  if (!ETHEREUM_VRF_CHAIN_IDS.has(chainId)) return 60n;
  return (
    BigInt(requestConfirmations) * ETHEREUM_SLOT_SECONDS +
    VRF_FULFILLMENT_BUFFER_SECONDS
  );
}

export function assertVrfRequestTimeout(
  chainId: number,
  requestConfirmations: number,
  requestTimeout: bigint,
): void {
  const minimum = minimumVrfRequestTimeout(chainId, requestConfirmations);
  if (requestTimeout < minimum) {
    throw new Error(
      `randomness.requestTimeout must be at least ${minimum} seconds for ${requestConfirmations} confirmations on chainId ${chainId}`,
    );
  }
}
