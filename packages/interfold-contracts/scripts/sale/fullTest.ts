// SPDX-License-Identifier: LGPL-3.0-only
import path from "path";

import { bidClaim } from "./bidClaim";
import { arg, connect, networkName } from "./cli";
import { deployFromPlan } from "./deploy";
import {
  deployMockBondingRegistryProxy,
  deployMockInitializerFactory,
  deployMockLbpStrategy,
  deployMockLiquidityLauncher,
  deploySaleDeployer,
} from "./deployContracts";
import { deploymentPath, planPath, saleDir, writeJson } from "./files";
import { buildSalePlan, printPlan } from "./plan";
import { makeTemplateConfig } from "./template";
import { validateDeployment } from "./validate";
import { address } from "./values";

export async function actionFullTest(): Promise<void> {
  const { ethers } = await connect();
  const network = await ethers.provider.getNetwork();
  if (
    network.chainId === 1n &&
    process.env.ALLOW_MAINNET_FULL_TEST !== "true"
  ) {
    throw new Error("Refusing to run mock full-test on mainnet");
  }
  const [operator] = await ethers.getSigners();
  const operatorAddress = await operator.getAddress();

  const safeInput = arg("safe") ?? process.env.SAFE_ADDRESS;
  const safe = safeInput ? address(safeInput, "safe") : operatorAddress;
  const registry = await deployMockBondingRegistryProxy(ethers, safe);
  const saleDeployer = await deploySaleDeployer(ethers, safe);
  const latest = await ethers.provider.getBlock("latest");
  if (!latest) throw new Error("Could not read latest block");

  const local = network.chainId === 31337n || network.chainId === 1337n;
  const mockInitializerFactory = local
    ? await deployMockInitializerFactory(ethers)
    : undefined;
  const mockLiquidityLauncher = local
    ? await deployMockLiquidityLauncher(ethers)
    : undefined;
  const mockLbpStrategy =
    local && mockInitializerFactory
      ? await deployMockLbpStrategy(ethers, mockInitializerFactory)
      : undefined;

  const name = arg("name") ?? `${networkName()}-sale-dry-run-${Date.now()}`;
  const config = makeTemplateConfig({
    name,
    chainId: Number(network.chainId),
    safe,
    saleDeployer,
    bondingRegistry: registry.proxy,
    currentBlock: BigInt(latest.number),
    currentTimestamp: BigInt(latest.timestamp),
  });
  if (local && mockLiquidityLauncher && mockLbpStrategy) {
    config.lbp!.liquidityLauncher = mockLiquidityLauncher;
    config.lbp!.strategy = mockLbpStrategy;
    config.auction.fundsRecipient = mockLbpStrategy;
  }
  const configFile = path.join(saleDir, `${name}.config.json`);
  writeJson(configFile, config);

  const plan = await buildSalePlan(ethers, config);
  const planFile = planPath(config);
  writeJson(planFile, plan);
  printPlan(plan, planFile);

  const deployment = await deployFromPlan(ethers, config, plan);
  deployment.bondingRegistryProxyAdmin = registry.proxyAdmin;
  writeJson(deploymentPath(config), deployment);

  if (safe === operatorAddress) {
    const fold = await ethers.getContractAt("InterfoldToken", deployment.fold);
    await (await fold.acceptOwnership()).wait();
    console.log(`Accepted FOLD ownership: ${deployment.fold}`);
    await (await fold.setClaimSource(deployment.auction)).wait();
    console.log(`Set FOLD claim source: ${deployment.auction}`);
  } else {
    console.log(
      `FOLD ownership is pending Safe activation. Execute the Safe batch from ${safe}: acceptOwnership(), setClaimSource(), and Predicate hook setup if applicable.`,
    );
  }

  await validateDeployment(
    ethers,
    config,
    deployment,
    plan,
    safe !== operatorAddress,
  );
  await bidClaim(ethers, config, deployment);

  console.log(`
Full Sepolia/local rehearsal complete
  config:     ${configFile}
  plan:       ${planFile}
  deployment: ${deploymentPath(config)}
  FOLD:       ${deployment.fold}
  auction:    ${deployment.auction}
`);
}
