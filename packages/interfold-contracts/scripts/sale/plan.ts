// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";
import fs from "fs";

import { planPath, readJson } from "./files";
import type { HardhatEthers, SaleConfigFile, SalePlan } from "./types";
import { CCA_INITIALIZER_FACTORY_ABI, LBP_STRATEGY_ABI } from "./uniswap";
import {
  address,
  buildFoldInitCode,
  deriveNoMoreLocks,
  encodeAuctionConfigData,
  encodeLauncherSalt,
  encodeLbpConfigData,
  encodeMigratorParameters,
  encodeMigratorSalt,
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

  const factoryNonce = await ethers.provider.getTransactionCount(
    config.saleDeployer,
  );
  const predictedFold = ethersLib.getCreateAddress({
    from: config.saleDeployer,
    nonce: BigInt(factoryNonce),
  });
  const auctionParams = toAuctionParameters(config.auction);
  const saleAmount = BigInt(config.saleAmount);
  if (saleAmount > (1n << 128n) - 1n) {
    throw new Error("saleAmount exceeds uint128 max");
  }

  const launchMode = "lbp";
  let predictedAuction: string;
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
    token: predictedFold,
    currency: auctionParams.currency,
  });
  const migratorConfigData = encodeMigratorParameters(migratorParams);
  const lbpConfigData = encodeLbpConfigData(migratorParams, ccaConfigData);
  const launcherSalt = encodeLauncherSalt(config.saleDeployer, config.ccaSalt);
  const initializerSalt = encodeMigratorSalt(launcherSalt, migratorParams);

  const cca = new ethersLib.Contract(
    initializerFactory,
    CCA_INITIALIZER_FACTORY_ABI,
    ethers.provider,
  );
  predictedAuction = address(
    await cca["getAddress(address,uint256,bytes,bytes32,address)"](
      predictedFold,
      saleAmount,
      ccaConfigData,
      initializerSalt,
      lbpStrategy,
    ),
    "predictedAuction",
  );

  const saleLabel = ethersLib.encodeBytes32String(config.saleLabel);
  const lbpSaleConfig: SalePlan["lbpSaleConfig"] = {
    liquidityLauncher,
    lbpStrategy,
    expectedAuction: predictedAuction,
    auctionAmount: saleAmount.toString(),
    reservedTokenAmountForLP: reservedTokenAmountForLP.toString(),
    distributionSalt: config.ccaSalt,
    lbpConfigData,
    saleLabel,
    foldInitCodeHash: "",
  };
  const lbpPlan: SalePlan["lbp"] = {
    initializerFactory,
    positionManager,
    poolManager,
    distributionAmount: distributionAmount.toString(),
    launcherSalt,
    initializerSalt,
    migratorConfigData,
    migratorParams,
  };

  const foldFactory = await ethers.getContractFactory("InterfoldToken");
  const foldInitCode = buildFoldInitCode({
    creationCode: foldFactory.bytecode,
    initialOwner: config.saleDeployer,
    ccaStart,
    ccaEnd,
    noMoreLocks,
    claimSource: predictedAuction,
    bondingRegistry: config.fold.bondingRegistry,
  });
  const foldInitCodeHash = ethersLib.keccak256(foldInitCode);
  if (lbpSaleConfig) {
    lbpSaleConfig.foldInitCodeHash = foldInitCodeHash;
  }

  const plan: SalePlan = {
    name: config.name,
    chainId,
    launchMode,
    saleDeployer: config.saleDeployer,
    safe: config.safe,
    factoryNonce,
    initializerFactory,
    liquidityLauncher,
    lbpStrategy,
    predictedFold,
    predictedAuction,
    fold: {
      initialOwner: config.saleDeployer,
      ccaStart: ccaStart.toString(),
      ccaEnd: ccaEnd.toString(),
      noMoreLocks: noMoreLocks.toString(),
      claimSource: predictedAuction,
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
  factoryNonce:  ${plan.factoryNonce}
  liquidityLauncher:${plan.liquidityLauncher}
  lbpStrategy:   ${plan.lbpStrategy}
  initializerFactory: ${plan.initializerFactory}
  FOLD:          ${plan.predictedFold}
  CCA auction:   ${plan.predictedAuction}
  LP reserve:    ${plan.lbpSaleConfig.reservedTokenAmountForLP}
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
