// SPDX-License-Identifier: LGPL-3.0-only
import type { MetaTransactionData } from "@safe-global/types-kit";

import type { connect } from "./cli";

export type HardhatEthers = Awaited<ReturnType<typeof connect>>["ethers"];

export interface FoldTokenConfig {
  ccaStart: string;
  ccaEnd: string;
  noMoreLocks?: string;
  bondingRegistry: string;
}

export interface AuctionConfig {
  currency: string;
  tokensRecipient: string;
  fundsRecipient: string;
  preSaleStartTimestamp?: string;
  startTimestamp?: string;
  auctionStartTimestamp?: string;
  auctionEndTimestamp?: string;
  endTimestamp?: string;
  claimTimestamp?: string;
  startBlock: string;
  endBlock: string;
  claimBlock: string;
  tickSpacing: string;
  tickSpacingQ96?: string;
  validationHook: string;
  floorPriceEthPerFold?: string;
  tickSpacingPercentOfFloor?: string;
  floorPrice: string;
  floorPriceQ96?: string;
  requiredRaiseEth?: string;
  requiredCurrencyRaised: string;
  requiredCurrencyRaisedWei?: string;
  auctionStepsData: string;
  generated?: {
    startBlock?: string;
    endBlock?: string;
    claimBlock?: string;
    floorPriceQ96?: string;
    tickSpacingQ96?: string;
    requiredCurrencyRaisedWei?: string;
    auctionStepsData?: string;
  };
}

export interface PredicateHookConfig {
  registry: string;
  policyID: string;
  address?: string;
  requireSenderIsOwner?: boolean;
}

export type LaunchMode = "lbp";

export interface PoolParametersConfig {
  fee: string;
  tickSpacing: string;
  hook: string;
}

export interface LbpUniswapConfig {
  liquidityLauncher?: string;
  lbpStrategy?: string;
}

export interface LbpRecipientConfig {
  proceedsRecipient?: string;
  lpPositionRecipient?: string;
}

export interface LbpAdvancedConfig {
  positionDefinitions?: string;
}

export interface LbpConfig {
  liquidityLauncher: string;
  strategy: string;
  uniswap?: LbpUniswapConfig;
  migrationDelayBlocks?: string;
  migrationBlock: string;
  lpAllocationPercent?: string;
  reservedTokenAmountForLP: string;
  reservedTokenAmountForLPWei?: string;
  recipient: string;
  positionRecipient: string;
  recipients?: LbpRecipientConfig;
  poolFee?: string;
  poolTickSpacing?: string;
  poolHook?: string;
  pool: PoolParametersConfig;
  positionDefinitions: string;
  lpAllocationSchedule: string;
  advanced?: LbpAdvancedConfig;
  generated?: {
    migrationBlock?: string;
    reservedTokenAmountForLPWei?: string;
    lpAllocationSchedule?: string;
  };
}

export interface SaleConfigFile {
  name: string;
  chainId: number;
  launchMode?: LaunchMode;
  saleDeployer: string;
  safe: string;
  saleAmountFold?: string;
  saleAmountWei?: string;
  saleAmount: string;
  ccaSalt: string;
  saleLabel: string;
  fold: FoldTokenConfig;
  auction: AuctionConfig;
  lbp?: LbpConfig;
  predicateHook?: PredicateHookConfig;
}

export interface SaleInfraFile {
  chainId: number;
  safe: string;
  saleDeployer: string;
  ccaSalt: string;
  bondingRegistryProxy: string;
  bondingRegistryImplementation: string;
  bondingRegistryProxyAdmin: string;
  validationHook?: string;
  predicateRegistry?: string;
  predicatePolicyID?: string;
  predicateRequireSenderIsOwner?: boolean;
}

export interface AuctionParameters {
  currency: string;
  tokensRecipient: string;
  fundsRecipient: string;
  startBlock: bigint;
  endBlock: bigint;
  claimBlock: bigint;
  tickSpacing: bigint;
  validationHook: string;
  floorPrice: bigint;
  requiredCurrencyRaised: bigint;
  auctionStepsData: string;
}

export interface PoolParameters {
  fee: bigint;
  tickSpacing: bigint;
  hook: string;
}

export interface MigratorParameters {
  token: string;
  currency: string;
  migrationBlock: bigint;
  reservedTokenAmountForLP: bigint;
  recipient: string;
  positionRecipient: string;
  poolParameters: PoolParameters;
  positionDefinitions: string;
  lpAllocationSchedule: string;
}

export interface SalePlan {
  name: string;
  chainId: number;
  launchMode?: LaunchMode;
  saleDeployer: string;
  safe: string;
  initializerFactory: string;
  liquidityLauncher: string;
  lbpStrategy: string;
  fold: {
    initialOwner: string;
    ccaStart: string;
    ccaEnd: string;
    noMoreLocks: string;
    bondingRegistry: string;
  };
  auction: AuctionParameters;
  lbpSaleConfig: {
    liquidityLauncher: string;
    lbpStrategy: string;
    ccaStart: string;
    ccaEnd: string;
    noMoreLocks: string;
    bondingRegistry: string;
    auctionAmount: string;
    reservedTokenAmountForLP: string;
    distributionSalt: string;
    currency: string;
    migrationBlock: string;
    recipient: string;
    positionRecipient: string;
    poolParameters: PoolParameters;
    positionDefinitions: string;
    lpAllocationSchedule: string;
    auctionConfigData: string;
    saleLabel: string;
    foldInitCodeHash: string;
  };
  lbp: {
    initializerFactory: string;
    positionManager: string;
    poolManager: string;
    distributionAmount: string;
    launcherSalt: string;
    migratorParams: MigratorParameters;
  };
  foldInitCode: string;
  sourceConfigHash?: string;
  configHash?: string;
  configDigest?: string;
}

export interface DeploymentFile {
  name: string;
  chainId: number;
  txHash: string;
  blockNumber: number;
  operator: string;
  safe: string;
  saleDeployer: string;
  launchMode?: LaunchMode;
  fold: string;
  auction: string;
  uniswapAuctionUrl?: string;
  bondingRegistry: string;
  bondingRegistryProxyAdmin?: string;
  initializerFactory: string;
  liquidityLauncher?: string;
  lbpStrategy?: string;
  reservedTokenAmountForLP?: string;
  migrationBlock?: string;
  validationHook?: string;
  predicateRegistry?: string;
  predicatePolicyID?: string;
  predicateRequireSenderIsOwner?: boolean;
  testBidId?: string;
  safeProposal?: SafeProposal;
}

export interface SafeProposal {
  safeTxHash: string;
  safeAddress: string;
  proposer: string;
  nonce: number;
  transactionCount: number;
  origin: string;
  url?: string;
  proposedAt: string;
}

export interface SafeAction {
  description: string;
  transaction: MetaTransactionData;
}

export interface SafeTransactionFallbackFile {
  name: string;
  chainId: number;
  safe: string;
  origin: string;
  createdAt: string;
  builderFile: string;
  transactions: Array<{
    description: string;
    to: string;
    value: string;
    data: string;
    operation: number;
  }>;
}

export interface SafeTransactionBuilderFile {
  version: "1.0";
  chainId: string;
  createdAt: number;
  meta: {
    name: string;
    description: string;
    txBuilderVersion: string;
    createdFromSafeAddress: string;
  };
  transactions: Array<{
    to: string;
    value: string;
    data: string;
    operation: number;
    contractMethod: null;
    contractInputsValues: null;
  }>;
}
