// SPDX-License-Identifier: LGPL-3.0-only

const UNISWAP_CHAIN_SLUGS: Record<number, string> = {
  1: "ethereum",
  11155111: "ethereum_sepolia",
};

export function uniswapAuctionUrl(chainId: number, auction: string): string {
  const slug = UNISWAP_CHAIN_SLUGS[chainId] ?? String(chainId);
  return `https://app.uniswap.org/explore/auctions/${slug}/${auction}`;
}
