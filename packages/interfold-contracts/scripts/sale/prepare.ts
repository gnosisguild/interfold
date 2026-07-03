// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";
import fs from "fs";

import { syncSaleInfraRecords } from "../deploymentRecords";
import { arg, connect, hasFlag, networkName } from "./cli";
import { ZERO } from "./constants";
import {
  deployMockBondingRegistryProxy,
  deployPredicateValidationHook,
  deploySaleDeployer,
} from "./deployContracts";
import {
  configPath,
  infraPath,
  nextAvailablePath,
  readJson,
  saleNameFromConfigPath,
  writeJson,
} from "./files";
import {
  applyDerivedConfigFields,
  applyReadableConfigFields,
  ccaPriceConfig,
  optionalTimestampInput,
} from "./schedule";
import {
  defaultLbpStrategy,
  makeDefaultLbpConfig,
  makeTemplateConfig,
  resolvePredicateHookInput,
} from "./template";
import type { SaleConfigFile, SaleInfraFile } from "./types";
import { LBP_STRATEGY_ADDRESSES } from "./uniswap";
import { address, normalizeSaleConfig, requireContract } from "./values";

type PrepareInputConfig = Partial<SaleConfigFile>;

function applyPrepareOverrides(config: SaleConfigFile): void {
  const saleAmountFold = arg("sale-amount-fold") ?? config.saleAmountFold;
  if (saleAmountFold?.trim()) {
    config.saleAmountFold = saleAmountFold;
  }

  const saleAmountOverride = arg("sale-amount");
  if (saleAmountOverride?.trim()) config.saleAmount = saleAmountOverride;

  const presaleStart =
    optionalTimestampInput("presale-start", "INTERFOLD_CCA_START") ??
    optionalTimestampInput("cca-start-timestamp", "INTERFOLD_CCA_START");
  if (presaleStart !== undefined) {
    config.fold.ccaStart = presaleStart.toString();
    config.auction.preSaleStartTimestamp = presaleStart.toString();
    config.auction.startTimestamp = presaleStart.toString();
  }

  const auctionStart =
    optionalTimestampInput("auction-start") ??
    optionalTimestampInput("auction-start-timestamp");
  if (auctionStart !== undefined) {
    config.auction.auctionStartTimestamp = auctionStart.toString();
  }

  const auctionEnd =
    optionalTimestampInput("auction-end", "INTERFOLD_CCA_END") ??
    optionalTimestampInput("cca-end-timestamp", "INTERFOLD_CCA_END");
  if (auctionEnd !== undefined) {
    config.fold.ccaEnd = auctionEnd.toString();
    config.auction.auctionEndTimestamp = auctionEnd.toString();
    config.auction.endTimestamp = auctionEnd.toString();
  }

  const claimTimestamp =
    optionalTimestampInput("claim-timestamp") ??
    optionalTimestampInput("auction-claim-timestamp");
  if (claimTimestamp !== undefined) {
    config.auction.claimTimestamp = claimTimestamp.toString();
  }

  config.auction.floorPriceEthPerFold =
    arg("floor-price-eth-per-fold") ??
    config.auction.floorPriceEthPerFold ??
    "0.000012";
  config.auction.tickSpacingPercentOfFloor =
    arg("tick-spacing-percent-of-floor") ??
    config.auction.tickSpacingPercentOfFloor ??
    "1";
  const { floorPrice, tickSpacing } = ccaPriceConfig({
    floorPriceEthPerFold: config.auction.floorPriceEthPerFold,
    tickSpacingPercentOfFloor: config.auction.tickSpacingPercentOfFloor,
  });
  config.auction.floorPrice = floorPrice.toString();
  config.auction.tickSpacing = tickSpacing.toString();

  const requiredCurrencyRaised = arg("required-currency-raised") ?? undefined;
  const requiredRaiseEth = arg("required-raise-eth");
  if (requiredRaiseEth !== undefined) {
    config.auction.requiredRaiseEth = requiredRaiseEth;
  }

  if (config.lbp) {
    config.lbp.lpAllocationPercent =
      arg("lp-allocation-percent") ?? config.lbp.lpAllocationPercent ?? "25";
    config.lbp.migrationDelayBlocks =
      arg("migration-delay-blocks") ?? config.lbp.migrationDelayBlocks ?? "20";
    config.lbp.poolFee = arg("pool-fee") ?? config.lbp.poolFee;
    config.lbp.poolTickSpacing =
      arg("pool-tick-spacing") ?? config.lbp.poolTickSpacing;
    config.lbp.poolHook = arg("pool-hook") ?? config.lbp.poolHook;
    if (arg("reserved-token-amount-for-lp")) {
      config.lbp.reservedTokenAmountForLP = arg(
        "reserved-token-amount-for-lp",
      )!;
    }
  }
  applyReadableConfigFields(config);
  if (saleAmountOverride?.trim()) config.saleAmount = saleAmountOverride;
  if (requiredCurrencyRaised !== undefined) {
    config.auction.requiredCurrencyRaised = requiredCurrencyRaised;
  }
  if (config.lbp && arg("reserved-token-amount-for-lp")) {
    config.lbp.reservedTokenAmountForLP = arg("reserved-token-amount-for-lp")!;
  }
}

function shouldForkExistingConfig(config?: SaleConfigFile): boolean {
  if (!config) return false;
  return config.saleDeployer !== ZERO && config.fold.bondingRegistry !== ZERO;
}

async function hasCode(
  provider: ethersLib.Provider,
  target: string,
): Promise<boolean> {
  return target !== ZERO && (await provider.getCode(target)) !== "0x";
}

function isKnownLbpStrategyForAnotherChain(
  strategy: string,
  chainId: number,
): boolean {
  const normalized = strategy.toLowerCase();
  return Object.entries(LBP_STRATEGY_ADDRESSES).some(
    ([knownChainId, knownStrategy]) =>
      Number(knownChainId) !== chainId &&
      knownStrategy.toLowerCase() === normalized,
  );
}

export function saleConfigJson(config: SaleConfigFile): unknown {
  return {
    name: config.name,
    chainId: config.chainId,
    saleDeployer: config.saleDeployer,
    safe: config.safe,
    launchMode: config.launchMode,
    saleAmountFold:
      config.saleAmountFold ?? ethersLib.formatEther(config.saleAmount),
    saleAmountWei: config.saleAmount,
    ccaSalt: config.ccaSalt,
    ...(config.saleLabel !== "cca-sale" ? { saleLabel: config.saleLabel } : {}),
    fold: config.fold,
    auction: {
      currency: config.auction.currency,
      tokensRecipient: config.auction.tokensRecipient,
      fundsRecipient: config.auction.fundsRecipient,
      preSaleStartTimestamp: config.auction.preSaleStartTimestamp,
      auctionStartTimestamp: config.auction.auctionStartTimestamp,
      auctionEndTimestamp: config.auction.auctionEndTimestamp,
      validationHook: config.auction.validationHook,
      floorPriceEthPerFold: config.auction.floorPriceEthPerFold,
      tickSpacingPercentOfFloor: config.auction.tickSpacingPercentOfFloor,
      requiredRaiseEth: config.auction.requiredRaiseEth,
      generated: {
        startBlock: config.auction.startBlock,
        endBlock: config.auction.endBlock,
        claimBlock: config.auction.claimBlock,
        floorPriceQ96: config.auction.floorPrice,
        tickSpacingQ96: config.auction.tickSpacing,
        requiredCurrencyRaisedWei: config.auction.requiredCurrencyRaised,
        auctionStepsData: config.auction.auctionStepsData,
      },
    },
    lbp: config.lbp
      ? {
          uniswap: {
            liquidityLauncher: config.lbp.liquidityLauncher,
            lbpStrategy: config.lbp.strategy,
          },
          migrationDelayBlocks: config.lbp.migrationDelayBlocks,
          lpAllocationPercent: config.lbp.lpAllocationPercent,
          recipients: {
            proceedsRecipient: config.lbp.recipient,
            lpPositionRecipient: config.lbp.positionRecipient,
          },
          pool: config.lbp.pool,
          advanced: {
            positionDefinitions: config.lbp.positionDefinitions,
          },
          generated: {
            migrationBlock: config.lbp.migrationBlock,
            reservedTokenAmountForLPWei: config.lbp.reservedTokenAmountForLP,
            lpAllocationSchedule: config.lbp.lpAllocationSchedule,
          },
        }
      : undefined,
    predicateHook: config.predicateHook,
  };
}

export async function actionPrepare(): Promise<void> {
  const { ethers } = await connect();
  const network = await ethers.provider.getNetwork();
  const chainId = Number(network.chainId);
  const [operator] = await ethers.getSigners();
  const operatorAddress = await operator.getAddress();
  const local = chainId === 31337 || chainId === 1337;

  const requestedFile = configPath(false);
  const requestedFileExists = fs.existsSync(requestedFile);
  const rawExistingConfig = requestedFileExists
    ? readJson<PrepareInputConfig>(requestedFile)
    : undefined;
  const existingChainId = rawExistingConfig?.chainId;
  const existingConfig = rawExistingConfig
    ? normalizeSaleConfig(
        {
          ...rawExistingConfig,
          name: saleNameFromConfigPath(requestedFile),
        },
        {
          file: requestedFile,
          allowMissingInfra: true,
        },
      )
    : undefined;

  const safeInput =
    arg("safe") ?? process.env.SAFE_ADDRESS ?? existingConfig?.safe;
  if (!safeInput && !local && !hasFlag("allow-eoa-safe")) {
    throw new Error("SAFE_ADDRESS or --safe is required outside localhost.");
  }
  const safe = address(safeInput ?? operatorAddress, "safe");
  if (!local && !hasFlag("allow-eoa-safe")) {
    await requireContract(ethers.provider, safe, "safe");
  }
  const shouldFork = shouldForkExistingConfig(existingConfig);
  const file = shouldFork ? nextAvailablePath(requestedFile) : requestedFile;
  const predicateHookInput = resolvePredicateHookInput(existingConfig);

  const registry = await deployMockBondingRegistryProxy(ethers, safe);
  const saleDeployer = await deploySaleDeployer(ethers, safe);
  let predicateHookAddress = predicateHookInput?.address;
  if (predicateHookInput && !predicateHookAddress) {
    predicateHookAddress = await deployPredicateValidationHook(ethers, {
      owner: safe,
      registry: predicateHookInput.registry,
      policyID: predicateHookInput.policyID,
      requireSenderIsOwner: predicateHookInput.requireSenderIsOwner ?? true,
    });
  }

  const latest = await ethers.provider.getBlock("latest");
  if (!latest) throw new Error("Could not read latest block");

  const preparedName = requestedFileExists
    ? (arg("name") ?? saleNameFromConfigPath(file))
    : (arg("name") ?? `${networkName()}-fold-cca`);
  const config =
    existingConfig ??
    makeTemplateConfig({
      name: preparedName,
      chainId,
      safe,
      saleDeployer,
      bondingRegistry: registry.proxy,
      currentBlock: BigInt(latest.number),
      currentTimestamp: BigInt(latest.timestamp),
    });

  config.name = preparedName;
  config.chainId = chainId;
  config.safe = safe;
  config.saleDeployer = saleDeployer;
  config.ccaSalt = ethersLib.id(`${config.name}:${chainId}:${Date.now()}`);
  config.fold.bondingRegistry = registry.proxy;
  config.launchMode = "lbp";
  delete (config as SaleConfigFile & { ccaFactory?: string }).ccaFactory;
  config.lbp ??= makeDefaultLbpConfig({
    chainId,
    safe,
    endBlock: BigInt(config.auction.endBlock),
    saleAmount: BigInt(config.saleAmount),
  });
  const copiedFromDifferentChain =
    existingChainId !== undefined && existingChainId !== chainId;
  const lbpStrategyOverride = arg("lbp-strategy") ?? process.env.LBP_STRATEGY;
  if (
    config.lbp &&
    (copiedFromDifferentChain ||
      lbpStrategyOverride ||
      isKnownLbpStrategyForAnotherChain(config.lbp.strategy, chainId) ||
      !(await hasCode(ethers.provider, config.lbp.strategy)))
  ) {
    config.lbp.strategy = address(
      lbpStrategyOverride ?? defaultLbpStrategy(chainId),
      "lbp.strategy",
    );
  }
  applyPrepareOverrides(config);
  config.auction.tokensRecipient = safe;
  config.auction.fundsRecipient = config.lbp.strategy;
  config.lbp.recipient = safe;
  config.lbp.positionRecipient = safe;
  if (predicateHookInput && predicateHookAddress) {
    config.auction.validationHook = predicateHookAddress;
    if (predicateHookInput.registry !== ZERO && predicateHookInput.policyID) {
      config.predicateHook = {
        registry: predicateHookInput.registry,
        policyID: predicateHookInput.policyID,
        address: predicateHookAddress,
        requireSenderIsOwner: predicateHookInput.requireSenderIsOwner ?? true,
      };
    }
  }
  applyDerivedConfigFields(config, {
    currentBlock: BigInt(latest.number),
    currentTimestamp: BigInt(latest.timestamp),
  });

  writeJson(file, saleConfigJson(config));
  const infra: SaleInfraFile = {
    chainId,
    safe,
    saleDeployer,
    ccaSalt: config.ccaSalt,
    bondingRegistryProxy: registry.proxy,
    bondingRegistryImplementation: registry.implementation,
    bondingRegistryProxyAdmin: registry.proxyAdmin,
    validationHook: predicateHookAddress,
    predicateRegistry:
      predicateHookInput?.registry === ZERO
        ? undefined
        : predicateHookInput?.registry,
    predicatePolicyID: predicateHookInput?.policyID || undefined,
    predicateRequireSenderIsOwner: predicateHookInput?.requireSenderIsOwner,
  };
  const infraFile = infraPath(config.name);
  writeJson(infraFile, infra);
  syncSaleInfraRecords(infra, {
    chain: networkName(),
    blockNumber: await ethers.provider.getBlockNumber(),
  });

  console.log(`
Prepared sale infrastructure
  mode:                         LiquidityLauncher / LBPStrategy
  safe:                         ${safe}
  saleDeployer:                 ${saleDeployer}
  MockBondingRegistry impl:     ${registry.implementation}
  bondingRegistry proxy:        ${registry.proxy}
  bondingRegistry ProxyAdmin:   ${registry.proxyAdmin}
  liquidityLauncher:            ${config.lbp?.liquidityLauncher ?? ZERO}
  lbpStrategy:                  ${config.lbp?.strategy ?? ZERO}
  validationHook:               ${predicateHookAddress ?? ZERO}
  config:                       ${file}
  infra:                        ${infraFile}
${shouldFork ? `  forkedFrom:                  ${requestedFile}\n` : ""}

Review the config schedule and economics, then run --action plan.
`);
}
