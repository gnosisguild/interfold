// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";

import { encodeSchedule, generateSchedule } from "../ccaSchedule";
import { arg, hasFlag } from "./cli";
import { DAY, DEFAULT_SALE_AMOUNT, ZERO, abi } from "./constants";
import type { PredicateHookConfig, SaleConfigFile } from "./types";
import {
  DEFAULT_CCA_PRICE_TICK_SPACING,
  DEFAULT_LP_ALLOCATION_RATE_MPS,
  LBP_STRATEGY_ADDRESSES,
  LIQUIDITY_LAUNCHER_ADDRESS,
  UNISWAP_V4_MEDIUM_FEE,
  UNISWAP_V4_MEDIUM_TICK_SPACING,
} from "./uniswap";
import { address } from "./values";

const DEFAULT_SECONDS_PER_BLOCK = 12n;

function optionalBigIntInput(
  cliName: string,
  envName?: string,
): bigint | undefined {
  const value = arg(cliName) ?? (envName ? process.env[envName] : undefined);
  if (!value?.trim()) return undefined;
  return BigInt(value);
}

function ceilDiv(value: bigint, divisor: bigint): bigint {
  return (value + divisor - 1n) / divisor;
}

function secondsPerBlock(): bigint {
  const value = optionalBigIntInput(
    "seconds-per-block",
    "AUCTION_SECONDS_PER_BLOCK",
  );
  const resolved = value ?? DEFAULT_SECONDS_PER_BLOCK;
  if (resolved <= 0n) throw new Error("seconds-per-block must be > 0");
  return resolved;
}

function blockForTimestamp(opts: {
  timestamp: bigint;
  currentBlock: bigint;
  currentTimestamp: bigint;
  secondsPerBlock: bigint;
}): bigint {
  if (opts.timestamp <= opts.currentTimestamp) {
    throw new Error(
      `auction timestamp ${opts.timestamp} must be greater than current timestamp ${opts.currentTimestamp}`,
    );
  }
  const blocksFromNow = ceilDiv(
    opts.timestamp - opts.currentTimestamp,
    opts.secondsPerBlock,
  );
  return opts.currentBlock + (blocksFromNow < 2n ? 2n : blocksFromNow);
}

export function defaultPositionDefinitions(): string {
  return abi.encode(
    [
      "tuple(int24 offsetLower,int24 offsetUpper,uint24 weight,address overridePositionRecipient)[]",
    ],
    [[]],
  );
}

export function defaultLpAllocationSchedule(): string {
  return abi.encode(
    ["tuple(uint128 lowerThreshold,uint24 rate)[]"],
    [
      [
        [
          0n,
          BigInt(
            arg("lp-allocation-rate-mps") ??
              DEFAULT_LP_ALLOCATION_RATE_MPS.toString(),
          ),
        ],
      ],
    ],
  );
}

export function defaultLbpStrategy(chainId: number): string {
  return (
    arg("lbp-strategy") ??
    process.env.LBP_STRATEGY ??
    LBP_STRATEGY_ADDRESSES[chainId] ??
    ZERO
  );
}

export function makeDefaultLbpConfig(opts: {
  chainId: number;
  safe: string;
  endBlock: bigint;
}) {
  const strategy = defaultLbpStrategy(opts.chainId);
  return {
    liquidityLauncher:
      arg("liquidity-launcher") ??
      process.env.LIQUIDITY_LAUNCHER ??
      LIQUIDITY_LAUNCHER_ADDRESS,
    strategy,
    migrationBlock: (
      opts.endBlock + BigInt(arg("migration-delay-blocks") ?? "20")
    ).toString(),
    reservedTokenAmountForLP:
      arg("reserved-token-amount-for-lp") ??
      ethersLib.parseEther("100").toString(),
    recipient: opts.safe,
    positionRecipient: opts.safe,
    pool: {
      fee: arg("pool-fee") ?? UNISWAP_V4_MEDIUM_FEE.toString(),
      tickSpacing:
        arg("pool-tick-spacing") ?? UNISWAP_V4_MEDIUM_TICK_SPACING.toString(),
      hook: arg("pool-hook") ?? ZERO,
    },
    positionDefinitions: defaultPositionDefinitions(),
    lpAllocationSchedule: defaultLpAllocationSchedule(),
  };
}

export function makeTemplateConfig(opts: {
  name: string;
  chainId: number;
  safe: string;
  saleDeployer: string;
  bondingRegistry: string;
  currentBlock: bigint;
  currentTimestamp: bigint;
}): SaleConfigFile {
  const explicitCcaStart = optionalBigIntInput(
    "cca-start-timestamp",
    "INTERFOLD_CCA_START",
  );
  const explicitCcaEnd = optionalBigIntInput(
    "cca-end-timestamp",
    "INTERFOLD_CCA_END",
  );
  const offsetSeconds = BigInt(arg("cca-offset-seconds") ?? String(DAY));
  const durationSeconds = BigInt(
    arg("cca-duration-seconds") ?? String(7n * DAY),
  );
  const ccaStart = explicitCcaStart ?? opts.currentTimestamp + offsetSeconds;
  const ccaEnd = explicitCcaEnd ?? ccaStart + durationSeconds;
  if (ccaEnd <= ccaStart) {
    throw new Error("cca-end-timestamp must be after cca-start-timestamp");
  }

  const explicitAuctionStart = optionalBigIntInput("auction-start-timestamp");
  const explicitAuctionEnd = optionalBigIntInput("auction-end-timestamp");
  const deriveAuctionBlocks =
    hasFlag("derive-auction-blocks") ||
    explicitAuctionStart !== undefined ||
    explicitAuctionEnd !== undefined ||
    explicitCcaStart !== undefined ||
    explicitCcaEnd !== undefined;
  const blockTime = secondsPerBlock();
  const startBlock = deriveAuctionBlocks
    ? blockForTimestamp({
        timestamp: explicitAuctionStart ?? ccaStart,
        currentBlock: opts.currentBlock,
        currentTimestamp: opts.currentTimestamp,
        secondsPerBlock: blockTime,
      })
    : opts.currentBlock + 2n;
  const endBlock = deriveAuctionBlocks
    ? blockForTimestamp({
        timestamp: explicitAuctionEnd ?? ccaEnd,
        currentBlock: opts.currentBlock,
        currentTimestamp: opts.currentTimestamp,
        secondsPerBlock: blockTime,
      })
    : startBlock + BigInt(arg("auction-duration-blocks") ?? "40");
  if (endBlock <= startBlock) {
    throw new Error("derived auction endBlock must be after startBlock");
  }
  const auctionBlocks = Number(endBlock - startBlock);
  const auctionStepsData = encodeSchedule(
    generateSchedule({
      auctionBlocks: auctionBlocks - 1,
      prebidBlocks: 0,
      numSteps: Math.min(12, Math.max(1, auctionBlocks - 1)),
      finalBlockPct: 0.3,
      alpha: 1.2,
    }),
  );
  const floorPrice = "4295000000";
  const lbp = makeDefaultLbpConfig({
    chainId: opts.chainId,
    safe: opts.safe,
    endBlock,
  });
  return {
    name: opts.name,
    chainId: opts.chainId,
    launchMode: "lbp",
    safe: opts.safe,
    saleDeployer: opts.saleDeployer,
    saleAmount: arg("sale-amount") ?? DEFAULT_SALE_AMOUNT,
    ccaSalt: ethersLib.id(`${opts.name}:${opts.chainId}:${Date.now()}`),
    saleLabel: arg("sale-label") ?? "cca-sale",
    fold: {
      ccaStart: ccaStart.toString(),
      ccaEnd: ccaEnd.toString(),
      noMoreLocks: "",
      bondingRegistry: opts.bondingRegistry,
    },
    auction: {
      currency: "ETH",
      tokensRecipient: opts.safe,
      fundsRecipient: lbp.strategy,
      startBlock: startBlock.toString(),
      endBlock: endBlock.toString(),
      claimBlock: (endBlock + 1n).toString(),
      tickSpacing: DEFAULT_CCA_PRICE_TICK_SPACING.toString(),
      validationHook: ZERO,
      floorPrice,
      requiredCurrencyRaised: "0",
      auctionStepsData,
    },
    lbp,
  };
}

function nonZero(value?: string): string | undefined {
  if (!value?.trim()) return undefined;
  return value === ZERO ? undefined : value;
}

export function resolvePredicateHookInput(
  config?: SaleConfigFile,
): PredicateHookConfig | undefined {
  const addressInput =
    arg("predicate-hook") ??
    arg("validation-hook") ??
    nonZero(config?.predicateHook?.address) ??
    nonZero(config?.auction.validationHook);
  const registryInput =
    arg("predicate-registry") ??
    process.env.PREDICATE_REGISTRY ??
    config?.predicateHook?.registry;
  const policyID =
    arg("predicate-policy-id") ??
    process.env.PREDICATE_POLICY_ID ??
    config?.predicateHook?.policyID;

  if (!addressInput && !registryInput && !policyID) return undefined;

  const requireSenderIsOwner = hasFlag("predicate-allow-delegated-owner")
    ? false
    : (config?.predicateHook?.requireSenderIsOwner ?? true);

  if (!addressInput && (!registryInput || !policyID)) {
    throw new Error(
      "Predicate hook deployment requires --predicate-registry and --predicate-policy-id.",
    );
  }
  if (registryInput && !policyID) {
    throw new Error("Predicate hook config requires a policy ID.");
  }

  return {
    registry: registryInput
      ? address(registryInput, "predicateHook.registry")
      : ZERO,
    policyID: policyID ?? "",
    address: addressInput
      ? address(addressInput, "predicateHook.address")
      : undefined,
    requireSenderIsOwner,
  };
}
