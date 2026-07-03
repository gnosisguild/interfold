// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";

import { syncSaleDeploymentRecords } from "../deploymentRecords";
import { getProxyAdmin } from "../proxy";
import { connect, hasFlag, networkName } from "./cli";
import { ZERO } from "./constants";
import {
  configPath,
  deploymentPath,
  jsonFileHash,
  readJson,
  writeJson,
} from "./files";
import { planConfigHash, readPlanForConfig } from "./plan";
import {
  buildSaleSafeActions,
  errorMessage,
  printSafeTransactionFallback,
  proposeSafeTransactions,
  proposeSaleSafeActions,
  safeActionsToTransactions,
  writeSafeTransactionFallback,
} from "./safe";
import type {
  DeploymentFile,
  HardhatEthers,
  SafeProposal,
  SaleConfigFile,
  SalePlan,
} from "./types";
import { CCA_AUCTION_ABI, LBP_STRATEGY_ABI } from "./uniswap";
import { uniswapAuctionUrl } from "./urls";
import {
  address,
  assertEq,
  lbpSaleConfigStruct,
  loadConfig,
  requireContract,
} from "./values";

function parseDeployedFold(
  deployer: Awaited<ReturnType<HardhatEthers["getContractAt"]>>,
  receipt: { logs: Array<{ topics: ReadonlyArray<string>; data: string }> },
): string {
  const event = receipt.logs
    .map((log) => {
      try {
        return deployer.interface.parseLog(log);
      } catch {
        return null;
      }
    })
    .find((parsed) => parsed?.name === "SaleDeployed");
  return address(event?.args?.fold as string, "SaleDeployed.fold");
}

function parseAuctionFromLbpLogs(
  plan: SalePlan,
  receipt: {
    logs: Array<{
      address: string;
      topics: ReadonlyArray<string>;
      data: string;
    }>;
  },
): string {
  const lbpInterface = new ethersLib.Interface(LBP_STRATEGY_ABI);
  const strategy = plan.lbpStrategy.toLowerCase();
  const event = receipt.logs
    .filter((log) => log.address.toLowerCase() === strategy)
    .map((log) => {
      try {
        return lbpInterface.parseLog(log);
      } catch {
        return null;
      }
    })
    .find((parsed) => parsed?.name === "InitializerCreated");
  return address(
    event?.args?.initializer as string,
    "LBPStrategy.InitializerCreated.initializer",
  );
}

async function validateFreshAuction(
  ethers: HardhatEthers,
  config: SaleConfigFile,
  fold: string,
  auction: string,
): Promise<void> {
  await requireContract(ethers.provider, auction, "auction");
  const cca = new ethersLib.Contract(auction, CCA_AUCTION_ABI, ethers.provider);
  assertEq("auction.token", await cca.token(), fold);
  assertEq("auction.totalSupply", await cca.totalSupply(), config.saleAmount);
}

export async function deployFromPlan(
  ethers: HardhatEthers,
  config: SaleConfigFile,
  plan: SalePlan,
): Promise<DeploymentFile> {
  if (!plan.lbp || !plan.lbpSaleConfig) {
    throw new Error(
      "Plan was generated for the removed direct CCA path. Run --action plan again with the official LiquidityLauncher/LBP config.",
    );
  }

  const deployer = await ethers.getContractAt(
    "InterfoldTokenSaleDeployer",
    config.saleDeployer,
  );
  const [operator] = await ethers.getSigners();
  const operatorAddress = await operator.getAddress();

  console.log(`Submitting deploySaleWithLiquidityLauncher for ${config.name}`);
  console.log(`  mode:             LiquidityLauncher / LBPStrategy`);
  console.log(`  FOLD:             discovered after deploy`);
  console.log(
    `  auction:          discovered from LBPStrategy.InitializerCreated`,
  );

  const tx = await deployer.deploySaleWithLiquidityLauncher(
    lbpSaleConfigStruct(plan),
    plan.foldInitCode,
  );
  console.log(`  tx: ${tx.hash}`);
  const receipt = await tx.wait();
  if (!receipt) throw new Error("deploySale transaction was not mined");

  const fold = parseDeployedFold(deployer, receipt);
  const auction = parseAuctionFromLbpLogs(plan, receipt);
  await validateFreshAuction(ethers, config, fold, auction);

  const deployment: DeploymentFile = {
    name: config.name,
    chainId: config.chainId,
    txHash: tx.hash,
    blockNumber: receipt.blockNumber,
    operator: operatorAddress,
    safe: config.safe,
    saleDeployer: config.saleDeployer,
    launchMode: "lbp",
    fold,
    auction,
    uniswapAuctionUrl: uniswapAuctionUrl(config.chainId, auction),
    bondingRegistry: config.fold.bondingRegistry,
    bondingRegistryProxyAdmin: await getProxyAdmin(
      ethers.provider,
      config.fold.bondingRegistry,
    ),
    initializerFactory: plan.initializerFactory,
    liquidityLauncher: plan.liquidityLauncher,
    lbpStrategy: plan.lbpStrategy,
    reservedTokenAmountForLP: plan.lbpSaleConfig.reservedTokenAmountForLP,
    migrationBlock: plan.lbp.migratorParams.migrationBlock.toString(),
    validationHook:
      config.auction.validationHook === ZERO
        ? undefined
        : config.auction.validationHook,
    predicateRegistry: config.predicateHook?.registry,
    predicatePolicyID: config.predicateHook?.policyID,
    predicateRequireSenderIsOwner: config.predicateHook?.requireSenderIsOwner,
  };
  writeJson(deploymentPath(config), deployment);
  syncSaleDeploymentRecords(deployment, plan, {
    chain: networkName(),
    blockNumber: receipt.blockNumber,
  });

  const safeOrigin = `Interfold ${config.name} sale Safe activation`;
  const safeActions = buildSaleSafeActions(config, deployment);
  const safeFallback = writeSafeTransactionFallback(
    config,
    safeActions,
    safeOrigin,
  );
  if (hasFlag("propose-safe")) {
    try {
      const proposal = await proposeSafeTransactions(
        config,
        safeActionsToTransactions(safeActions),
        safeOrigin,
      );
      deployment.safeProposal = proposal;
      writeJson(deploymentPath(config), deployment);
      console.log(`
Safe transaction proposed
  hash: ${proposal.safeTxHash}
  nonce: ${proposal.nonce}
  url:  ${proposal.url ?? "(open the Safe UI pending queue)"}
`);
    } catch (error) {
      printSafeTransactionFallback(
        config,
        safeFallback,
        `Safe API proposal failed: ${errorMessage(error)}`,
      );
    }
  } else {
    printSafeTransactionFallback(
      config,
      safeFallback,
      "run again with --propose-safe to propose this batch through the Safe SDK",
    );
  }
  console.log(`
Sale deployed
  FOLD:    ${fold}
  auction: ${auction}
  Uniswap: ${deployment.uniswapAuctionUrl}
  mode:    LiquidityLauncher / LBPStrategy
  tx:      ${tx.hash}
  config hash: ${planConfigHash(plan)}
`);
  return deployment;
}

export async function actionDeploy(): Promise<void> {
  const { ethers } = await connect();
  const configFile = configPath();
  const config = loadConfig(configFile);
  const plan = await readPlanForConfig(config);
  if (
    plan.sourceConfigHash &&
    plan.sourceConfigHash !== jsonFileHash(configFile)
  ) {
    throw new Error(
      "Plan is stale because the sale config changed. Run --action plan again, inspect the new plan, then deploy.",
    );
  }
  await deployFromPlan(ethers, config, plan);
}

export async function actionAcceptOwnership(): Promise<void> {
  const { ethers } = await connect();
  const config = loadConfig();
  const deployment = readJson<DeploymentFile>(deploymentPath(config));
  const fold = await ethers.getContractAt("InterfoldToken", deployment.fold);
  const tx = await fold.acceptOwnership();
  await tx.wait();
  console.log(`Accepted FOLD ownership: ${deployment.fold}`);
}

export async function actionProposeSafe(): Promise<void> {
  const config = loadConfig();
  const deployment = readJson<DeploymentFile>(deploymentPath(config));
  let proposal: SafeProposal;
  try {
    proposal = await proposeSaleSafeActions(config, deployment);
  } catch (error) {
    const origin = `Interfold ${config.name} sale Safe activation`;
    const safeFallback = writeSafeTransactionFallback(
      config,
      buildSaleSafeActions(config, deployment),
      origin,
    );
    printSafeTransactionFallback(
      config,
      safeFallback,
      `Safe API proposal failed: ${errorMessage(error)}`,
    );
    process.exitCode = 1;
    return;
  }
  deployment.safeProposal = proposal;
  writeJson(deploymentPath(config), deployment);

  console.log(`
Safe transaction proposed
  hash: ${proposal.safeTxHash}
  nonce: ${proposal.nonce}
  txs:  ${proposal.transactionCount}
  url:  ${proposal.url ?? "(open the Safe UI pending queue)"}
`);
}
