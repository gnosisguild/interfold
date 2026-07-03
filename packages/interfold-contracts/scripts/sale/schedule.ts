// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";

import { encodeSchedule, generateSchedule } from "../ccaSchedule";
import { arg } from "./cli";
import { abi } from "./constants";
import type { SaleConfigFile } from "./types";

const DEFAULT_SECONDS_PER_BLOCK = 12n;
const Q96 = 1n << 96n;
const MIN_CCA_FLOOR_PRICE = (1n << 32n) + 1n;

export function optionalBigIntInput(
  cliName: string,
  envName?: string,
): bigint | undefined {
  const value = arg(cliName) ?? (envName ? process.env[envName] : undefined);
  if (!value?.trim()) return undefined;
  return BigInt(value);
}

export function optionalTimestampInput(
  cliName: string,
  envName?: string,
): bigint | undefined {
  const value = arg(cliName) ?? (envName ? process.env[envName] : undefined);
  if (!value?.trim()) return undefined;
  return parseTimestamp(value, cliName);
}

export function parseTimestamp(value: string, label: string): bigint {
  const trimmed = value.trim();
  if (/^\d+$/.test(trimmed)) return BigInt(trimmed);

  const parsed = Date.parse(trimmed);
  if (Number.isNaN(parsed)) {
    throw new Error(
      `${label} must be Unix seconds or an ISO date like 2026-07-08T10:00:00-04:00`,
    );
  }
  return BigInt(Math.floor(parsed / 1000));
}

function ceilDiv(value: bigint, divisor: bigint): bigint {
  return (value + divisor - 1n) / divisor;
}

export function secondsPerBlock(): bigint {
  const value = optionalBigIntInput(
    "seconds-per-block",
    "AUCTION_SECONDS_PER_BLOCK",
  );
  const resolved = value ?? DEFAULT_SECONDS_PER_BLOCK;
  if (resolved <= 0n) throw new Error("seconds-per-block must be > 0");
  return resolved;
}

export function blockForTimestamp(opts: {
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

function parseDecimal(
  value: string,
  label: string,
): {
  numerator: bigint;
  denominator: bigint;
} {
  const trimmed = value.trim();
  if (!/^\d+(\.\d+)?$/.test(trimmed)) {
    throw new Error(`${label} must be a positive decimal string`);
  }
  const [whole, fraction = ""] = trimmed.split(".");
  const denominator = 10n ** BigInt(fraction.length);
  const numerator = BigInt(`${whole}${fraction}`);
  if (numerator <= 0n) throw new Error(`${label} must be > 0`);
  return { numerator, denominator };
}

export function decimalToQ96(value: string, label: string): bigint {
  const { numerator, denominator } = parseDecimal(value, label);
  return (numerator * Q96) / denominator;
}

export function percentToMps(value: string, label: string): bigint {
  const { numerator, denominator } = parseDecimal(value, label);
  if (numerator > 100n * denominator) {
    throw new Error(`${label} must be <= 100`);
  }
  return (numerator * 10_000_000n) / (100n * denominator);
}

export function percentFraction(
  value: string,
  label: string,
): {
  numerator: bigint;
  denominator: bigint;
} {
  const { numerator, denominator } = parseDecimal(value, label);
  if (numerator > 100n * denominator) {
    throw new Error(`${label} must be <= 100`);
  }
  return { numerator, denominator: 100n * denominator };
}

export function ccaPriceConfig(opts: {
  floorPriceEthPerFold: string;
  tickSpacingPercentOfFloor: string;
}): { floorPrice: bigint; tickSpacing: bigint } {
  const rawFloorPrice = decimalToQ96(
    opts.floorPriceEthPerFold,
    "floorPriceEthPerFold",
  );
  if (rawFloorPrice < MIN_CCA_FLOOR_PRICE) {
    throw new Error(
      `floorPriceEthPerFold encodes below the CCA minimum floor price ${MIN_CCA_FLOOR_PRICE}`,
    );
  }

  const tickFraction = percentFraction(
    opts.tickSpacingPercentOfFloor,
    "tickSpacingPercentOfFloor",
  );
  let tickSpacing =
    (rawFloorPrice * tickFraction.numerator) / tickFraction.denominator;
  if (tickSpacing < 2n) tickSpacing = 2n;

  const floorPrice = rawFloorPrice - (rawFloorPrice % tickSpacing);
  if (floorPrice < MIN_CCA_FLOOR_PRICE) {
    throw new Error("floorPrice rounded below the CCA minimum");
  }
  return { floorPrice, tickSpacing };
}

export function lpAllocationSchedule(rateMps: bigint): string {
  return abi.encode(
    ["tuple(uint128 lowerThreshold,uint24 rate)[]"],
    [[[0n, rateMps]]],
  );
}

export function tokenReserveForLp(saleAmount: bigint, rateMps: bigint): bigint {
  return (saleAmount * rateMps) / 10_000_000n;
}

export function applyReadableConfigFields(config: SaleConfigFile): void {
  config.auction.generated ??= {};
  if (config.saleAmountFold?.trim()) {
    config.saleAmount = ethersLib.parseEther(config.saleAmountFold).toString();
    config.saleAmountWei = config.saleAmount;
  }

  if (config.auction.requiredRaiseEth?.trim()) {
    config.auction.requiredCurrencyRaised = ethersLib
      .parseEther(config.auction.requiredRaiseEth)
      .toString();
    config.auction.requiredCurrencyRaisedWei =
      config.auction.requiredCurrencyRaised;
    config.auction.generated.requiredCurrencyRaisedWei =
      config.auction.requiredCurrencyRaised;
  }

  if (config.auction.floorPriceEthPerFold?.trim()) {
    const { floorPrice, tickSpacing } = ccaPriceConfig({
      floorPriceEthPerFold: config.auction.floorPriceEthPerFold,
      tickSpacingPercentOfFloor:
        config.auction.tickSpacingPercentOfFloor ?? "1",
    });
    config.auction.floorPrice = floorPrice.toString();
    config.auction.tickSpacing = tickSpacing.toString();
    config.auction.floorPriceQ96 = config.auction.floorPrice;
    config.auction.tickSpacingQ96 = config.auction.tickSpacing;
    config.auction.generated.floorPriceQ96 = config.auction.floorPrice;
    config.auction.generated.tickSpacingQ96 = config.auction.tickSpacing;
  }

  if (config.lbp?.lpAllocationPercent?.trim()) {
    config.lbp.generated ??= {};
    const rateMps = percentToMps(
      config.lbp.lpAllocationPercent,
      "lbp.lpAllocationPercent",
    );
    config.lbp.lpAllocationSchedule = lpAllocationSchedule(rateMps);
    config.lbp.reservedTokenAmountForLP = tokenReserveForLp(
      BigInt(config.saleAmount),
      rateMps,
    ).toString();
    config.lbp.reservedTokenAmountForLPWei =
      config.lbp.reservedTokenAmountForLP;
    config.lbp.generated.reservedTokenAmountForLPWei =
      config.lbp.reservedTokenAmountForLP;
    config.lbp.generated.lpAllocationSchedule = config.lbp.lpAllocationSchedule;
  }

  if (config.lbp) {
    config.lbp.pool.fee = config.lbp.poolFee ?? config.lbp.pool.fee;
    config.lbp.pool.tickSpacing =
      config.lbp.poolTickSpacing ?? config.lbp.pool.tickSpacing;
    config.lbp.pool.hook = config.lbp.poolHook ?? config.lbp.pool.hook;
  }
}

export function auctionStepsDataForWindow(opts: {
  startBlock: bigint;
  auctionStartBlock: bigint;
  endBlock: bigint;
}): string {
  if (opts.auctionStartBlock < opts.startBlock) {
    throw new Error("auctionStartBlock must be >= startBlock");
  }
  if (opts.endBlock <= opts.auctionStartBlock) {
    throw new Error("endBlock must be after auctionStartBlock");
  }

  const totalBlocks = opts.endBlock - opts.startBlock;
  const prebidBlocks = opts.auctionStartBlock - opts.startBlock;
  const auctionBlocks = Number(totalBlocks - prebidBlocks - 1n);
  if (auctionBlocks <= 0) {
    throw new Error("auction block window is too short");
  }

  return encodeSchedule(
    generateSchedule({
      auctionBlocks,
      prebidBlocks: Number(prebidBlocks),
      numSteps: Math.min(12, Math.max(1, auctionBlocks)),
      finalBlockPct: 0.3,
      alpha: 1.2,
    }),
  );
}

export function configSalt(name: string, chainId: number): string {
  return ethersLib.id(`${name}:${chainId}:${Date.now()}`);
}

function configTimestamp(
  value: string | undefined,
  label: string,
): bigint | undefined {
  if (!value?.trim()) return undefined;
  return parseTimestamp(value, label);
}

export function applyDerivedConfigFields(
  config: SaleConfigFile,
  opts: { currentBlock: bigint; currentTimestamp: bigint },
): void {
  const startTimestamp = configTimestamp(
    config.auction.preSaleStartTimestamp ??
      config.auction.startTimestamp ??
      config.fold.ccaStart,
    "auction.preSaleStartTimestamp",
  );
  const auctionStartTimestamp = configTimestamp(
    config.auction.auctionStartTimestamp,
    "auction.auctionStartTimestamp",
  );
  const endTimestamp = configTimestamp(
    config.auction.auctionEndTimestamp ??
      config.auction.endTimestamp ??
      config.fold.ccaEnd,
    "auction.auctionEndTimestamp",
  );
  const claimTimestamp = configTimestamp(
    config.auction.claimTimestamp,
    "auction.claimTimestamp",
  );

  if (startTimestamp && endTimestamp) {
    if (endTimestamp <= startTimestamp) {
      throw new Error(
        "auction.endTimestamp must be after auction.startTimestamp",
      );
    }
    if (auctionStartTimestamp) {
      if (
        auctionStartTimestamp < startTimestamp ||
        auctionStartTimestamp >= endTimestamp
      ) {
        throw new Error(
          "auction.auctionStartTimestamp must be within [startTimestamp, endTimestamp)",
        );
      }
    }

    const blockTime = secondsPerBlock();
    const startBlock = blockForTimestamp({
      timestamp: startTimestamp,
      currentBlock: opts.currentBlock,
      currentTimestamp: opts.currentTimestamp,
      secondsPerBlock: blockTime,
    });
    const auctionStartBlock = auctionStartTimestamp
      ? blockForTimestamp({
          timestamp: auctionStartTimestamp,
          currentBlock: opts.currentBlock,
          currentTimestamp: opts.currentTimestamp,
          secondsPerBlock: blockTime,
        })
      : startBlock;
    const endBlock = blockForTimestamp({
      timestamp: endTimestamp,
      currentBlock: opts.currentBlock,
      currentTimestamp: opts.currentTimestamp,
      secondsPerBlock: blockTime,
    });
    const migrationDelayBlocks =
      arg("migration-delay-blocks") ?? config.lbp?.migrationDelayBlocks ?? "20";
    const migrationBlock = config.lbp
      ? endBlock + BigInt(migrationDelayBlocks)
      : endBlock;
    const claimBlock = claimTimestamp
      ? blockForTimestamp({
          timestamp: claimTimestamp,
          currentBlock: opts.currentBlock,
          currentTimestamp: opts.currentTimestamp,
          secondsPerBlock: blockTime,
        })
      : migrationBlock;

    if (claimBlock < endBlock) {
      throw new Error(
        "auction.claimTimestamp must be at or after endTimestamp",
      );
    }
    if (config.lbp && claimBlock < migrationBlock) {
      throw new Error(
        `auction.claimTimestamp must be at or after LBP migrationBlock ${migrationBlock}`,
      );
    }

    config.fold.ccaStart = startTimestamp.toString();
    config.fold.ccaEnd = endTimestamp.toString();
    config.auction.preSaleStartTimestamp = startTimestamp.toString();
    config.auction.startTimestamp = startTimestamp.toString();
    config.auction.auctionStartTimestamp = (
      auctionStartTimestamp ?? startTimestamp
    ).toString();
    config.auction.auctionEndTimestamp = endTimestamp.toString();
    config.auction.endTimestamp = endTimestamp.toString();
    config.auction.startBlock = startBlock.toString();
    config.auction.endBlock = endBlock.toString();
    config.auction.claimBlock = claimBlock.toString();
    config.auction.auctionStepsData = auctionStepsDataForWindow({
      startBlock,
      auctionStartBlock,
      endBlock,
    });
    config.auction.generated ??= {};
    config.auction.generated.startBlock = config.auction.startBlock;
    config.auction.generated.endBlock = config.auction.endBlock;
    config.auction.generated.claimBlock = config.auction.claimBlock;
    config.auction.generated.auctionStepsData = config.auction.auctionStepsData;

    if (config.lbp) {
      config.lbp.generated ??= {};
      config.lbp.migrationDelayBlocks = migrationDelayBlocks;
      config.lbp.migrationBlock = migrationBlock.toString();
      config.lbp.generated.migrationBlock = config.lbp.migrationBlock;
    }
  }

  if (config.auction.floorPriceEthPerFold?.trim()) {
    const { floorPrice, tickSpacing } = ccaPriceConfig({
      floorPriceEthPerFold: config.auction.floorPriceEthPerFold,
      tickSpacingPercentOfFloor:
        config.auction.tickSpacingPercentOfFloor ?? "1",
    });
    config.auction.floorPrice = floorPrice.toString();
    config.auction.tickSpacing = tickSpacing.toString();
    config.auction.floorPriceQ96 = config.auction.floorPrice;
    config.auction.tickSpacingQ96 = config.auction.tickSpacing;
    config.auction.generated ??= {};
    config.auction.generated.floorPriceQ96 = config.auction.floorPrice;
    config.auction.generated.tickSpacingQ96 = config.auction.tickSpacing;
  }
}
