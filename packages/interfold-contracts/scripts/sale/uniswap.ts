// SPDX-License-Identifier: LGPL-3.0-only

export const CCA_VERSION = "v2.0.0";
export const LIQUIDITY_LAUNCHER_VERSION = "v3.0.0";

export const LIQUIDITY_LAUNCHER_ADDRESS =
  "0x00004c4ccc709Ef590F7C81102C0689F0263D4e9";

export const LBP_STRATEGY_ADDRESSES: Record<number, string> = {
  1: "0xb98766A35cdc28415be0767D4EA41e39fBA3e000",
  11155111: "0x3f37838651B5AD71D4e01Ec9745862A5D9DF2000",
};

export const DEFAULT_CCA_PRICE_TICK_SPACING = 100_000n;
export const UNISWAP_V4_MEDIUM_FEE = 3_000n;
export const UNISWAP_V4_MEDIUM_TICK_SPACING = 60n;
export const LP_ALLOCATION_RATE_DENOMINATOR_MPS = 10_000_000n;
export const DEFAULT_LP_ALLOCATION_RATE_MPS = 2_500_000n;

export const AUCTION_PARAMETERS_TUPLE =
  "tuple(" +
  "address currency," +
  "address tokensRecipient," +
  "address fundsRecipient," +
  "uint64 startBlock," +
  "uint64 endBlock," +
  "uint64 claimBlock," +
  "uint256 tickSpacing," +
  "address validationHook," +
  "uint256 floorPrice," +
  "uint128 requiredCurrencyRaised," +
  "bytes auctionStepsData" +
  ")";

export const MIGRATOR_PARAMETERS_TUPLE =
  "tuple(" +
  "address token," +
  "address currency," +
  "uint64 migrationBlock," +
  "uint128 reservedTokenAmountForLP," +
  "address recipient," +
  "address positionRecipient," +
  "tuple(uint24 fee,int24 tickSpacing,address hook) poolParameters," +
  "bytes positionDefinitions," +
  "bytes lpAllocationSchedule" +
  ")";

export const CCA_INITIALIZER_FACTORY_ABI = [
  "function create(address token,uint256 amount,bytes configData,bytes32 salt) returns (address)",
  "function getAddress(address token,uint256 amount,bytes configData,bytes32 salt,address sender) view returns (address)",
  "function protocolFeeController() view returns (address)",
];

export const LIQUIDITY_LAUNCHER_ABI = [
  "function distributeToken(address tokenAddress,tuple(address strategy,uint128 amount,bytes configData) distribution,bytes32 salt)",
  "event TokenDistributed(address indexed tokenAddress,address indexed strategy,uint256 amount)",
];

export const LBP_STRATEGY_ABI = [
  "function initializerFactory() view returns (address)",
  "function positionManager() view returns (address)",
  "function poolManager() view returns (address)",
  "function initializers(address initializer) view returns (tuple(address token,address currency,uint64 migrationBlock,uint128 reservedTokenAmountForLP,address recipient,address positionRecipient,tuple(uint24 fee,int24 tickSpacing,address hook) poolParameters,bytes positionDefinitions,bytes lpAllocationSchedule))",
  "event InitializerCreated(address indexed initializer,tuple(address token,address currency,uint64 migrationBlock,uint128 reservedTokenAmountForLP,address recipient,address positionRecipient,tuple(uint24 fee,int24 tickSpacing,address hook) poolParameters,bytes positionDefinitions,bytes lpAllocationSchedule) migrationParams)",
];

export const CCA_AUCTION_ABI = [
  "function token() view returns (address)",
  "function totalSupply() view returns (uint128)",
  "function tokensRecipient() view returns (address)",
  "function fundsRecipient() view returns (address)",
  "function currency() view returns (address)",
  "function startBlock() view returns (uint64)",
  "function endBlock() view returns (uint64)",
  "function claimBlock() view returns (uint64)",
  "function validationHook() view returns (address)",
  "event TokensReceived(uint128 totalSupply)",
  "function tokensReceived() view returns (bool)",
  "function isGraduated() view returns (bool)",
  "function currencyRaised() view returns (uint256)",
  "function checkpoint() returns (tuple(uint256 clearingPrice,uint224 currencyRaisedAtClearingPriceQ96X7,uint256 cumulativeMpsPerPrice,uint24 cumulativeMps,uint64 prev,uint64 next))",
  "function bids(uint256 bidId) view returns (tuple(uint64 startBlock,uint24 startCumulativeMps,uint64 exitedBlock,uint256 maxPrice,address owner,uint256 amountQ96,uint256 tokensFilled))",
  "function submitBid(uint256 maxPrice,uint128 amount,address owner,bytes hookData) payable returns (uint256 bidId)",
  "function exitBid(uint256 bidId)",
  "function exitPartiallyFilledBid(uint256 bidId,uint64 lastFullyFilledCheckpointBlock,uint64 outbidBlock)",
  "function claimTokens(uint256 bidId)",
  "event BidSubmitted(uint256 indexed id,address indexed owner,uint256 price,uint256 amount)",
  "function bid() payable",
  "function claim() returns (uint256)",
];
