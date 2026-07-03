// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";
import fs from "fs";

import { ZERO } from "./constants";
import { planPath, readJson } from "./files";
import { applyDerivedConfigFields } from "./schedule";
import type { HardhatEthers, SaleConfigFile, SalePlan } from "./types";
import { LBP_STRATEGY_ABI } from "./uniswap";
import {
  address,
  buildFoldInitCode,
  deriveNoMoreLocks,
  encodeAuctionConfigData,
  encodeLauncherSalt,
  lbpSaleConfigStruct,
  requireContract,
  resolveLbpStrategy,
  resolveLiquidityLauncher,
  toAuctionParameters,
  toMigratorParameters,
} from "./values";

export async function buildSalePlan(
  ethers: HardhatEthers,
  config: SaleConfigFile,
): Promise<SalePlan> {
  const network = await ethers.provider.getNetwork();
  const chainId = Number(network.chainId);
  if (chainId !== config.chainId) {
    throw new Error(
      `Connected chainId ${chainId} != config.chainId ${config.chainId}`,
    );
  }

  await requireContract(ethers.provider, config.saleDeployer, "saleDeployer");
  await requireContract(
    ethers.provider,
    config.fold.bondingRegistry,
    "fold.bondingRegistry",
  );

  const saleDeployer = await ethers.getContractAt(
    "InterfoldTokenSaleDeployer",
    config.saleDeployer,
  );
  const protocolAdmin = address(
    await saleDeployer.protocolAdmin(),
    "protocolAdmin",
  );
  if (protocolAdmin !== config.safe) {
    throw new Error(
      `saleDeployer.protocolAdmin mismatch: expected ${config.safe}, got ${protocolAdmin}`,
    );
  }

  const latest = await ethers.provider.getBlock("latest");
  if (!latest) throw new Error("Could not read latest block");
  applyDerivedConfigFields(config, {
    currentBlock: BigInt(latest.number),
    currentTimestamp: BigInt(latest.timestamp),
  });

  const ccaStart = BigInt(config.fold.ccaStart);
  const ccaEnd = BigInt(config.fold.ccaEnd);
  if (ccaStart <= BigInt(latest.timestamp)) {
    throw new Error(
      `fold.ccaStart (${ccaStart}) must be in the future; latest timestamp is ${latest.timestamp}`,
    );
  }
  if (ccaEnd <= ccaStart) {
    throw new Error("fold.ccaEnd must be after fold.ccaStart");
  }
  const noMoreLocks = deriveNoMoreLocks(ccaEnd, config.fold.noMoreLocks);

  const auctionParams = toAuctionParameters(config.auction);
  const saleAmount = BigInt(config.saleAmount);
  if (saleAmount > (1n << 128n) - 1n) {
    throw new Error("saleAmount exceeds uint128 max");
  }

  const launchMode = "lbp";
  const ccaConfigData = encodeAuctionConfigData(auctionParams);

  if (!config.lbp) throw new Error("lbp config is required");
  const liquidityLauncher = resolveLiquidityLauncher(config);
  const lbpStrategy = resolveLbpStrategy(config);
  await requireContract(
    ethers.provider,
    liquidityLauncher,
    "lbp.liquidityLauncher",
  );
  await requireContract(ethers.provider, lbpStrategy, "lbp.strategy");

  const strategy = new ethersLib.Contract(
    lbpStrategy,
    LBP_STRATEGY_ABI,
    ethers.provider,
  );
  const initializerFactory = address(
    await strategy.initializerFactory(),
    "lbp.initializerFactory",
  );
  const positionManager = address(
    await strategy.positionManager(),
    "lbp.positionManager",
  );
  const poolManager = address(await strategy.poolManager(), "lbp.poolManager");
  await requireContract(
    ethers.provider,
    initializerFactory,
    "lbp.initializerFactory",
  );

  if (auctionParams.fundsRecipient !== lbpStrategy) {
    throw new Error(
      `official LBP flow requires auction.fundsRecipient to be LBPStrategy ${lbpStrategy}; got ${auctionParams.fundsRecipient}`,
    );
  }

  const reservedTokenAmountForLP = BigInt(config.lbp.reservedTokenAmountForLP);
  const distributionAmount = saleAmount + reservedTokenAmountForLP;
  if (distributionAmount <= saleAmount) {
    throw new Error("lbp.reservedTokenAmountForLP must be > 0");
  }
  if (distributionAmount > (1n << 128n) - 1n) {
    throw new Error("saleAmount + reservedTokenAmountForLP exceeds uint128");
  }

  const migratorParams = toMigratorParameters(config.lbp, {
    token: ZERO,
    currency: auctionParams.currency,
  });
  if (auctionParams.claimBlock < migratorParams.migrationBlock) {
    throw new Error(
      `auction.claimBlock must be at or after lbp.migrationBlock (${migratorParams.migrationBlock}) so liquidity can migrate before or during claims open`,
    );
  }
  const launcherSalt = encodeLauncherSalt(config.saleDeployer, config.ccaSalt);

  const saleLabel = ethersLib.encodeBytes32String(config.saleLabel);
  const foldFactory = await ethers.getContractFactory("InterfoldToken");
  const foldInitCode = buildFoldInitCode({
    creationCode: foldFactory.bytecode,
    initialOwner: config.saleDeployer,
    ccaStart,
    ccaEnd,
    noMoreLocks,
    bondingRegistry: config.fold.bondingRegistry,
  });
  const foldInitCodeHash = ethersLib.keccak256(foldInitCode);
  const lbpSaleConfig: SalePlan["lbpSaleConfig"] = {
    liquidityLauncher,
    lbpStrategy,
    ccaStart: ccaStart.toString(),
    ccaEnd: ccaEnd.toString(),
    noMoreLocks: noMoreLocks.toString(),
    bondingRegistry: config.fold.bondingRegistry,
    auctionAmount: saleAmount.toString(),
    reservedTokenAmountForLP: reservedTokenAmountForLP.toString(),
    distributionSalt: config.ccaSalt,
    currency: auctionParams.currency,
    migrationBlock: migratorParams.migrationBlock.toString(),
    recipient: migratorParams.recipient,
    positionRecipient: migratorParams.positionRecipient,
    poolParameters: migratorParams.poolParameters,
    positionDefinitions: migratorParams.positionDefinitions,
    lpAllocationSchedule: migratorParams.lpAllocationSchedule,
    auctionConfigData: ccaConfigData,
    saleLabel,
    foldInitCodeHash,
  };
  const lbpPlan: SalePlan["lbp"] = {
    initializerFactory,
    positionManager,
    poolManager,
    distributionAmount: distributionAmount.toString(),
    launcherSalt,
    migratorParams,
  };

  const plan: SalePlan = {
    name: config.name,
    chainId,
    launchMode,
    saleDeployer: config.saleDeployer,
    safe: config.safe,
    initializerFactory,
    liquidityLauncher,
    lbpStrategy,
    fold: {
      initialOwner: config.saleDeployer,
      ccaStart: ccaStart.toString(),
      ccaEnd: ccaEnd.toString(),
      noMoreLocks: noMoreLocks.toString(),
      bondingRegistry: config.fold.bondingRegistry,
    },
    auction: auctionParams,
    lbpSaleConfig,
    lbp: lbpPlan,
    foldInitCode,
  };
  plan.configHash = await saleDeployer.hashLbpConfig(lbpSaleConfigStruct(plan));
  return plan;
}

export function printPlan(plan: SalePlan, planFile: string): void {
  console.log(`
Interfold sale plan
  config:        ${plan.name}
  chainId:       ${plan.chainId}
  safe:          ${plan.safe}
  mode:          LiquidityLauncher / LBPStrategy
  saleDeployer:  ${plan.saleDeployer}
  liquidityLauncher:${plan.liquidityLauncher}
  lbpStrategy:   ${plan.lbpStrategy}
  initializerFactory: ${plan.initializerFactory}
  FOLD:          discovered at deploy
  CCA auction:   discovered from LBPStrategy.InitializerCreated
  auction FOLD:  ${plan.lbpSaleConfig.auctionAmount}
  LP FOLD reserve:${plan.lbpSaleConfig.reservedTokenAmountForLP}
  CCA floorPrice:${plan.auction.floorPrice}
  CCA tickSpacing:${plan.auction.tickSpacing}
  required raise:${plan.auction.requiredCurrencyRaised}
  migrationBlock:${plan.lbp.migratorParams.migrationBlock}
  bondingRegistry proxy: ${plan.fold.bondingRegistry}
  FOLD timestamps: start=${plan.fold.ccaStart} end=${plan.fold.ccaEnd} noMoreLocks=${plan.fold.noMoreLocks}
  CCA blocks:    start=${plan.auction.startBlock} end=${plan.auction.endBlock} claim=${plan.auction.claimBlock}
  config hash:   ${planConfigHash(plan)}
  plan file:     ${planFile}
`);
}

export function planConfigHash(plan: SalePlan): string {
  const hash = plan.configHash ?? plan.configDigest;
  if (!hash) {
    throw new Error("Plan is missing configHash. Run --action plan again.");
  }
  return hash;
}

export async function readPlanForConfig(
  config: SaleConfigFile,
): Promise<SalePlan> {
  const file = planPath(config);
  if (!fs.existsSync(file)) {
    throw new Error(`Plan file not found: ${file}. Run --action plan first.`);
  }
  return readJson<SalePlan>(file);
}
