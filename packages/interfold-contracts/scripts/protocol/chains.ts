// SPDX-License-Identifier: LGPL-3.0-only

const SUPPORTED_VRF_CHAIN_IDS = new Set([1, 1_337, 31_337, 11_155_111]);

export function assertSupportedVrfChain(chainId: number): void {
  if (!SUPPORTED_VRF_CHAIN_IDS.has(chainId)) {
    throw new Error(
      `VRF sortition supports Ethereum mainnet, Sepolia, and local development chains only; received chainId ${chainId}`,
    );
  }
}
