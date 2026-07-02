// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";

import { decodeSchedule } from "../ccaSchedule";
import { arg } from "./cli";
import {
  FORTY_DAYS,
  FOUR_YEARS,
  MSG_SENDER_SENTINEL,
  ZERO,
  abi,
} from "./constants";
import { configPath, readJson } from "./files";
import type {
  AuctionConfig,
  AuctionParameters,
  LbpConfig,
  MigratorParameters,
  SaleConfigFile,
  SalePlan,
} from "./types";
import {
  AUCTION_PARAMETERS_TUPLE,
  CCA_VERSION,
  LBP_STRATEGY_ADDRESSES,
  LIQUIDITY_LAUNCHER_ADDRESS,
  MIGRATOR_PARAMETERS_TUPLE,
} from "./uniswap";

export function address(value: string, label: string): string {
  try {
    return ethersLib.getAddress(value);
  } catch {
    throw new Error(`${label} is not a valid address: ${value}`);
  }
}

export function requireBytes32(value: string, label: string): string {
  if (!/^0x[0-9a-fA-F]{64}$/.test(value)) {
    throw new Error(`${label} must be a 0x-prefixed bytes32`);
  }
  return value;
}

function validateAuctionStepsData(
  auctionStepsData: string,
  startBlock: bigint,
  endBlock: bigint,
): void {
  const schedule = decodeSchedule(auctionStepsData);
  const totalBlocks = schedule.reduce((sum, step) => sum + step.blockDelta, 0n);
  const windowBlocks = endBlock - startBlock;
  if (totalBlocks !== windowBlocks) {
    throw new Error(
      `auctionStepsData covers ${totalBlocks} blocks, but auction window is ${windowBlocks}`,
    );
  }
}

export function loadConfig(file = configPath()): SaleConfigFile {
  const config = readJson<SaleConfigFile>(file);
  const safeOverride = arg("safe") ?? process.env.SAFE_ADDRESS;
  const saleDeployerOverride = arg("sale-deployer");
  const bondingOverride = arg("bonding-registry");

  if (safeOverride && config.safe === ZERO) config.safe = safeOverride;
  if (saleDeployerOverride && config.saleDeployer === ZERO) {
    config.saleDeployer = saleDeployerOverride;
  }
  if (bondingOverride && config.fold.bondingRegistry === ZERO) {
    config.fold.bondingRegistry = bondingOverride;
  }

  validateConfig(config);
  return config;
}

export function validateConfig(config: SaleConfigFile): void {
  if (!config.name) throw new Error("Config name is required");
  config.launchMode = config.launchMode ?? "lbp";
  if (config.launchMode !== "lbp") {
    throw new Error(
      "Only launchMode=lbp is supported. The direct CCA factory path was removed; rerun --action prepare to generate an official LiquidityLauncher config.",
    );
  }
  const legacyVersion = (config as SaleConfigFile & { ccaVersion?: unknown })
    .ccaVersion;
  if (legacyVersion !== undefined && legacyVersion !== CCA_VERSION) {
    throw new Error(
      `ccaVersion is no longer configurable; remove it or set it to ${CCA_VERSION}`,
    );
  }
  delete (config as SaleConfigFile & { ccaVersion?: unknown }).ccaVersion;
  config.safe = address(config.safe, "safe");
  config.saleDeployer = address(config.saleDeployer, "saleDeployer");
  if (!config.lbp) throw new Error("lbp config is required");
  validateLbpConfig(config.lbp);
  config.fold.bondingRegistry = address(
    config.fold.bondingRegistry,
    "fold.bondingRegistry",
  );
  config.auction.tokensRecipient = address(
    config.auction.tokensRecipient,
    "auction.tokensRecipient",
  );
  config.auction.fundsRecipient = address(
    config.auction.fundsRecipient,
    "auction.fundsRecipient",
  );
  config.auction.validationHook = address(
    config.auction.validationHook || ZERO,
    "auction.validationHook",
  );
  if (config.predicateHook) {
    config.predicateHook.registry = address(
      config.predicateHook.registry,
      "predicateHook.registry",
    );
    if (config.predicateHook.address?.trim()) {
      config.predicateHook.address = address(
        config.predicateHook.address,
        "predicateHook.address",
      );
      if (config.auction.validationHook === ZERO) {
        config.auction.validationHook = config.predicateHook.address;
      }
    }
    if (!config.predicateHook.policyID?.trim()) {
      throw new Error(
        "predicateHook.policyID is required when predicateHook is set",
      );
    }
  }
  requireBytes32(config.ccaSalt, "ccaSalt");
  ethersLib.encodeBytes32String(config.saleLabel);
  BigInt(config.saleAmount);
  BigInt(config.fold.ccaStart);
  BigInt(config.fold.ccaEnd);
  if (config.fold.noMoreLocks?.trim()) BigInt(config.fold.noMoreLocks);
}

function validateBytes(value: string, label: string): string {
  if (!ethersLib.isHexString(value)) {
    throw new Error(`${label} must be 0x-prefixed hex bytes`);
  }
  return value;
}

function validateLbpConfig(config: LbpConfig): void {
  config.liquidityLauncher = address(
    config.liquidityLauncher || LIQUIDITY_LAUNCHER_ADDRESS,
    "lbp.liquidityLauncher",
  );
  config.strategy = address(config.strategy, "lbp.strategy");
  BigInt(config.migrationBlock);
  BigInt(config.reservedTokenAmountForLP);
  config.recipient = address(config.recipient, "lbp.recipient");
  config.positionRecipient = address(
    config.positionRecipient,
    "lbp.positionRecipient",
  );
  config.pool.hook = address(config.pool.hook || ZERO, "lbp.pool.hook");
  BigInt(config.pool.fee);
  BigInt(config.pool.tickSpacing);
  config.positionDefinitions = validateBytes(
    config.positionDefinitions,
    "lbp.positionDefinitions",
  );
  config.lpAllocationSchedule = validateBytes(
    config.lpAllocationSchedule,
    "lbp.lpAllocationSchedule",
  );

  abi.decode(
    [
      "tuple(int24 offsetLower,int24 offsetUpper,uint24 weight,address overridePositionRecipient)[]",
    ],
    config.positionDefinitions,
  );
  const brackets = abi.decode(
    ["tuple(uint128 lowerThreshold,uint24 rate)[]"],
    config.lpAllocationSchedule,
  )[0] as Array<{ lowerThreshold: bigint; rate: bigint }>;
  if (brackets.length === 0) {
    throw new Error(
      "lbp.lpAllocationSchedule must contain at least one bracket",
    );
  }
  let previous: bigint | undefined;
  for (const [index, bracket] of brackets.entries()) {
    if (index === 0 && bracket.lowerThreshold !== 0n) {
      throw new Error("lbp.lpAllocationSchedule first threshold must be 0");
    }
    if (previous !== undefined && bracket.lowerThreshold <= previous) {
      throw new Error("lbp.lpAllocationSchedule thresholds must increase");
    }
    if (bracket.rate === 0n || bracket.rate > 10_000_000n) {
      throw new Error("lbp.lpAllocationSchedule rates must be 1..10000000");
    }
    previous = bracket.lowerThreshold;
  }
}

export function resolveCurrency(currency: string): string {
  if (!currency || currency.toUpperCase() === "ETH") return ZERO;
  return address(currency, "auction.currency");
}

export function toAuctionParameters(config: AuctionConfig): AuctionParameters {
  const startBlock = BigInt(config.startBlock);
  const endBlock = BigInt(config.endBlock);
  const claimBlock = BigInt(config.claimBlock);
  if (endBlock <= startBlock) {
    throw new Error("auction.endBlock must be greater than auction.startBlock");
  }
  if (claimBlock < endBlock) {
    throw new Error("auction.claimBlock must be >= auction.endBlock");
  }
  const auctionStepsData = config.auctionStepsData || "0x";
  if (!ethersLib.isHexString(auctionStepsData)) {
    throw new Error("auction.auctionStepsData must be 0x-prefixed hex");
  }
  validateAuctionStepsData(auctionStepsData, startBlock, endBlock);
  return {
    currency: resolveCurrency(config.currency),
    tokensRecipient: address(config.tokensRecipient, "auction.tokensRecipient"),
    fundsRecipient: address(config.fundsRecipient, "auction.fundsRecipient"),
    startBlock,
    endBlock,
    claimBlock,
    tickSpacing: BigInt(config.tickSpacing),
    validationHook: address(
      config.validationHook || ZERO,
      "auction.validationHook",
    ),
    floorPrice: BigInt(config.floorPrice),
    requiredCurrencyRaised: BigInt(config.requiredCurrencyRaised),
    auctionStepsData,
  };
}

export function encodeAuctionConfigData(params: AuctionParameters): string {
  return abi.encode(
    [AUCTION_PARAMETERS_TUPLE],
    [
      [
        params.currency,
        params.tokensRecipient,
        params.fundsRecipient,
        params.startBlock,
        params.endBlock,
        params.claimBlock,
        params.tickSpacing,
        params.validationHook,
        params.floorPrice,
        params.requiredCurrencyRaised,
        params.auctionStepsData,
      ],
    ],
  );
}

export function resolveLiquidityLauncher(config: SaleConfigFile): string {
  return address(
    config.lbp?.liquidityLauncher ?? LIQUIDITY_LAUNCHER_ADDRESS,
    "lbp.liquidityLauncher",
  );
}

export function resolveLbpStrategy(config: SaleConfigFile): string {
  const fallback = LBP_STRATEGY_ADDRESSES[config.chainId];
  const value = config.lbp?.strategy ?? fallback;
  if (!value) {
    throw new Error(`No default LBPStrategy for chain ${config.chainId}`);
  }
  return address(value, "lbp.strategy");
}

export function deriveNoMoreLocks(ccaEnd: bigint, explicit?: string): bigint {
  if (explicit?.trim()) {
    const value = BigInt(explicit);
    const minimum = ccaEnd + FORTY_DAYS;
    if (value <= minimum) {
      throw new Error(
        `fold.noMoreLocks must be greater than ccaEnd + 40 days (${minimum})`,
      );
    }
    return value;
  }
  return ccaEnd + FORTY_DAYS + FOUR_YEARS;
}

export function buildFoldInitCode(opts: {
  creationCode: string;
  initialOwner: string;
  ccaStart: bigint;
  ccaEnd: bigint;
  noMoreLocks: bigint;
  claimSource: string;
  bondingRegistry: string;
}): string {
  const encodedCtor = abi.encode(
    ["address", "uint64", "uint64", "uint64", "address", "address"],
    [
      opts.initialOwner,
      opts.ccaStart,
      opts.ccaEnd,
      opts.noMoreLocks,
      opts.claimSource,
      opts.bondingRegistry,
    ],
  );
  return ethersLib.concat([opts.creationCode, encodedCtor]);
}

export function toMigratorParameters(
  config: LbpConfig,
  opts: { token: string; currency: string },
): MigratorParameters {
  return {
    token: opts.token,
    currency: opts.currency,
    migrationBlock: BigInt(config.migrationBlock),
    reservedTokenAmountForLP: BigInt(config.reservedTokenAmountForLP),
    recipient: address(config.recipient, "lbp.recipient"),
    positionRecipient: address(
      config.positionRecipient,
      "lbp.positionRecipient",
    ),
    poolParameters: {
      fee: BigInt(config.pool.fee),
      tickSpacing: BigInt(config.pool.tickSpacing),
      hook: address(config.pool.hook || ZERO, "lbp.pool.hook"),
    },
    positionDefinitions: validateBytes(
      config.positionDefinitions,
      "lbp.positionDefinitions",
    ),
    lpAllocationSchedule: validateBytes(
      config.lpAllocationSchedule,
      "lbp.lpAllocationSchedule",
    ),
  };
}

export function encodeMigratorParameters(params: MigratorParameters): string {
  return abi.encode(
    [MIGRATOR_PARAMETERS_TUPLE],
    [migratorParametersValue(params)],
  );
}

export function migratorParametersValue(params: MigratorParameters) {
  return [
    params.token,
    params.currency,
    params.migrationBlock,
    params.reservedTokenAmountForLP,
    params.recipient,
    params.positionRecipient,
    [
      params.poolParameters.fee,
      params.poolParameters.tickSpacing,
      params.poolParameters.hook,
    ],
    params.positionDefinitions,
    params.lpAllocationSchedule,
  ] as const;
}

export function encodeMigratorSalt(
  launcherSalt: string,
  params: MigratorParameters,
): string {
  return ethersLib.keccak256(
    abi.encode(
      ["bytes32", MIGRATOR_PARAMETERS_TUPLE],
      [launcherSalt, migratorParametersValue(params)],
    ),
  );
}

export function encodeLauncherSalt(caller: string, salt: string): string {
  return ethersLib.keccak256(
    abi.encode(["address", "bytes32"], [caller, salt]),
  );
}

export function encodeLbpConfigData(
  migratorParams: MigratorParameters,
  auctionConfigData: string,
): string {
  return abi.encode(
    [MIGRATOR_PARAMETERS_TUPLE, "bytes"],
    [migratorParametersValue(migratorParams), auctionConfigData],
  );
}

export function lbpSaleConfigStruct(plan: SalePlan) {
  if (!plan.lbpSaleConfig) {
    throw new Error(
      "Plan is missing lbpSaleConfig. Run --action plan again with the official LiquidityLauncher/LBP config.",
    );
  }
  return {
    liquidityLauncher: plan.lbpSaleConfig.liquidityLauncher,
    lbpStrategy: plan.lbpSaleConfig.lbpStrategy,
    expectedAuction: plan.lbpSaleConfig.expectedAuction,
    auctionAmount: BigInt(plan.lbpSaleConfig.auctionAmount),
    reservedTokenAmountForLP: BigInt(
      plan.lbpSaleConfig.reservedTokenAmountForLP,
    ),
    distributionSalt: plan.lbpSaleConfig.distributionSalt,
    lbpConfigData: plan.lbpSaleConfig.lbpConfigData,
    saleLabel: plan.lbpSaleConfig.saleLabel,
    foldInitCodeHash: plan.lbpSaleConfig.foldInitCodeHash,
  };
}

export function resolvedRecipient(value: string, sender: string): string {
  return value.toLowerCase() === MSG_SENDER_SENTINEL
    ? address(sender, "sender")
    : address(value, "recipient");
}

export async function codeAt(
  provider: ethersLib.Provider,
  target: string,
): Promise<string> {
  return provider.getCode(target);
}

export async function requireContract(
  provider: ethersLib.Provider,
  target: string,
  label: string,
): Promise<void> {
  const code = await codeAt(provider, target);
  if (code === "0x") throw new Error(`${label} has no code: ${target}`);
}

export async function deployedAddress(contract: {
  target?: unknown;
  getAddress?: () => Promise<string>;
}): Promise<string> {
  if (typeof contract.target === "string") {
    return address(contract.target, "contract");
  }
  if (contract.getAddress) {
    return address(await contract.getAddress(), "contract");
  }
  throw new Error("Could not determine deployed contract address");
}

export function assertEq(
  label: string,
  actual: unknown,
  expected: unknown,
): void {
  if (String(actual).toLowerCase() !== String(expected).toLowerCase()) {
    throw new Error(`${label}: expected ${expected}, got ${actual}`);
  }
  console.log(`  ok ${label}`);
}

export function formatFold(value: bigint | string): string {
  return `${ethersLib.formatUnits(value, 18)} FOLD`;
}

export async function optionalView<T>(
  label: string,
  read: () => Promise<T>,
): Promise<T | undefined> {
  try {
    return await read();
  } catch {
    console.log(`  skip ${label} (view not available)`);
    return undefined;
  }
}
