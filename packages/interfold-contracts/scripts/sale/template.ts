// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";

import { arg, hasFlag } from "./cli";
import { DAY, DEFAULT_SALE_AMOUNT, ZERO, abi } from "./constants";
import {
  auctionStepsDataForWindow,
  blockForTimestamp,
  ccaPriceConfig,
  configSalt,
  lpAllocationSchedule,
  optionalTimestampInput,
  percentToMps,
  secondsPerBlock,
  tokenReserveForLp,
} from "./schedule";
import type { PredicateHookConfig, SaleConfigFile } from "./types";
import {
  DEFAULT_LP_ALLOCATION_RATE_MPS,
  LBP_STRATEGY_ADDRESSES,
  LIQUIDITY_LAUNCHER_ADDRESS,
  UNISWAP_V4_MEDIUM_FEE,
  UNISWAP_V4_MEDIUM_TICK_SPACING,
} from "./uniswap";
import { address } from "./values";

export function defaultPositionDefinitions(): string {
  return abi.encode(
    [
      "tuple(int24 offsetLower,int24 offsetUpper,uint24 weight,address overridePositionRecipient)[]",
    ],
    [[]],
  );
}

export function defaultLpAllocationSchedule(): string {
  return lpAllocationSchedule(
    BigInt(
      arg("lp-allocation-rate-mps") ??
        (arg("lp-allocation-percent")
          ? percentToMps(arg("lp-allocation-percent")!, "lp-allocation-percent")
          : DEFAULT_LP_ALLOCATION_RATE_MPS
        ).toString(),
    ),
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
  saleAmount: bigint;
  lpAllocationPercent?: string;
}) {
  const strategy = defaultLbpStrategy(opts.chainId);
  const lpAllocationPercent =
    arg("lp-allocation-percent") ?? opts.lpAllocationPercent ?? "25";
  const lpRateMps = BigInt(
    arg("lp-allocation-rate-mps") ??
      percentToMps(lpAllocationPercent, "lp-allocation-percent").toString(),
  );
  return {
    liquidityLauncher:
      arg("liquidity-launcher") ??
      process.env.LIQUIDITY_LAUNCHER ??
      LIQUIDITY_LAUNCHER_ADDRESS,
    strategy,
    migrationDelayBlocks: arg("migration-delay-blocks") ?? "20",
    migrationBlock: (
      opts.endBlock + BigInt(arg("migration-delay-blocks") ?? "20")
    ).toString(),
    lpAllocationPercent,
    reservedTokenAmountForLP:
      arg("reserved-token-amount-for-lp") ??
      tokenReserveForLp(opts.saleAmount, lpRateMps).toString(),
    recipient: opts.safe,
    positionRecipient: opts.safe,
    pool: {
      fee: arg("pool-fee") ?? UNISWAP_V4_MEDIUM_FEE.toString(),
      tickSpacing:
        arg("pool-tick-spacing") ?? UNISWAP_V4_MEDIUM_TICK_SPACING.toString(),
      hook: arg("pool-hook") ?? ZERO,
    },
    poolFee: arg("pool-fee") ?? UNISWAP_V4_MEDIUM_FEE.toString(),
    poolTickSpacing:
      arg("pool-tick-spacing") ?? UNISWAP_V4_MEDIUM_TICK_SPACING.toString(),
    poolHook: arg("pool-hook") ?? ZERO,
    positionDefinitions: defaultPositionDefinitions(),
    lpAllocationSchedule: lpAllocationSchedule(lpRateMps),
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
  const explicitCcaStart =
    optionalTimestampInput("presale-start", "INTERFOLD_CCA_START") ??
    optionalTimestampInput("cca-start-timestamp", "INTERFOLD_CCA_START");
  const explicitAuctionStart = optionalTimestampInput("auction-start");
  const explicitCcaEnd =
    optionalTimestampInput("auction-end", "INTERFOLD_CCA_END") ??
    optionalTimestampInput("cca-end-timestamp", "INTERFOLD_CCA_END");
  const offsetSeconds = BigInt(arg("cca-offset-seconds") ?? String(DAY));
  const durationSeconds = BigInt(
    arg("cca-duration-seconds") ?? String(7n * DAY),
  );
  const ccaStart = explicitCcaStart ?? opts.currentTimestamp + offsetSeconds;
  const ccaEnd = explicitCcaEnd ?? ccaStart + durationSeconds;
  if (ccaEnd <= ccaStart) {
    throw new Error("cca-end-timestamp must be after cca-start-timestamp");
  }

  const explicitAuctionBlockStart =
    optionalTimestampInput("auction-start-timestamp") ?? explicitCcaStart;
  const explicitAuctionEnd =
    optionalTimestampInput("auction-end-timestamp") ?? explicitCcaEnd;
  const deriveAuctionBlocks =
    hasFlag("derive-auction-blocks") ||
    explicitAuctionBlockStart !== undefined ||
    explicitAuctionEnd !== undefined ||
    explicitAuctionStart !== undefined ||
    explicitCcaStart !== undefined ||
    explicitCcaEnd !== undefined;
  const blockTime = secondsPerBlock();
  const startBlock = deriveAuctionBlocks
    ? blockForTimestamp({
        timestamp: explicitAuctionBlockStart ?? ccaStart,
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
  const issuanceStartBlock =
    explicitAuctionStart && explicitAuctionStart > ccaStart
      ? blockForTimestamp({
          timestamp: explicitAuctionStart,
          currentBlock: opts.currentBlock,
          currentTimestamp: opts.currentTimestamp,
          secondsPerBlock: blockTime,
        })
      : startBlock;
  if (endBlock <= startBlock) {
    throw new Error("derived auction endBlock must be after startBlock");
  }
  const auctionStepsData = auctionStepsDataForWindow({
    startBlock,
    auctionStartBlock: issuanceStartBlock,
    endBlock,
  });
  const floorPriceEthPerFold = arg("floor-price-eth-per-fold") ?? "0.000012";
  const tickSpacingPercentOfFloor = arg("tick-spacing-percent-of-floor") ?? "1";
  const { floorPrice, tickSpacing } = ccaPriceConfig({
    floorPriceEthPerFold,
    tickSpacingPercentOfFloor,
  });
  const saleAmountFold = arg("sale-amount-fold");
  const saleAmount = saleAmountFold
    ? ethersLib.parseEther(saleAmountFold)
    : BigInt(arg("sale-amount") ?? DEFAULT_SALE_AMOUNT);
  const lbp = makeDefaultLbpConfig({
    chainId: opts.chainId,
    safe: opts.safe,
    endBlock,
    saleAmount,
    lpAllocationPercent: arg("lp-allocation-percent"),
  });
  const migrationBlock = BigInt(lbp.migrationBlock);
  return {
    name: opts.name,
    chainId: opts.chainId,
    launchMode: "lbp",
    safe: opts.safe,
    saleDeployer: opts.saleDeployer,
    saleAmountFold,
    saleAmount: saleAmount.toString(),
    ccaSalt: configSalt(opts.name, opts.chainId),
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
      preSaleStartTimestamp: ccaStart.toString(),
      startTimestamp: ccaStart.toString(),
      auctionStartTimestamp: (explicitAuctionStart ?? ccaStart).toString(),
      auctionEndTimestamp: ccaEnd.toString(),
      endTimestamp: ccaEnd.toString(),
      startBlock: startBlock.toString(),
      endBlock: endBlock.toString(),
      claimBlock: migrationBlock.toString(),
      tickSpacing: tickSpacing.toString(),
      validationHook: ZERO,
      floorPriceEthPerFold,
      tickSpacingPercentOfFloor,
      floorPrice: floorPrice.toString(),
      requiredRaiseEth: arg("required-raise-eth") ?? "400",
      requiredCurrencyRaised:
        arg("required-currency-raised") ??
        ethersLib.parseEther(arg("required-raise-eth") ?? "400").toString(),
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
  const cliAddressInput = arg("predicate-hook") ?? arg("validation-hook");
  const cliRegistryInput = arg("predicate-registry");
  const cliPolicyID = arg("predicate-policy-id");
  const configPredicateAddress =
    nonZero(config?.predicateHook?.address) ??
    nonZero(config?.auction.validationHook);
  const configPredicateRegistry = nonZero(config?.predicateHook?.registry);
  const configPolicyID = config?.predicateHook?.policyID;
  const shouldUseEnv = !config;
  const addressInput = cliAddressInput ?? configPredicateAddress;
  const registryInput =
    cliRegistryInput ??
    configPredicateRegistry ??
    (shouldUseEnv ? nonZero(process.env.PREDICATE_REGISTRY) : undefined);
  const policyID =
    cliPolicyID ??
    configPolicyID ??
    (shouldUseEnv ? process.env.PREDICATE_POLICY_ID : undefined);

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
