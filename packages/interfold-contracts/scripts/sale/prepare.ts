// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";
import fs from "fs";
import path from "path";

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
  nextAvailablePath,
  readJson,
  saleDir,
  saleNameFromConfigPath,
  writeJson,
} from "./files";
import {
  makeDefaultLbpConfig,
  makeTemplateConfig,
  resolvePredicateHookInput,
} from "./template";
import type { SaleConfigFile } from "./types";
import { address, requireContract } from "./values";

export async function actionPrepare(): Promise<void> {
  const { ethers } = await connect();
  const network = await ethers.provider.getNetwork();
  const chainId = Number(network.chainId);
  const [operator] = await ethers.getSigners();
  const operatorAddress = await operator.getAddress();
  const local = chainId === 31337 || chainId === 1337;
  const safeInput = arg("safe") ?? process.env.SAFE_ADDRESS;
  if (!safeInput && !local && !hasFlag("allow-eoa-safe")) {
    throw new Error("SAFE_ADDRESS or --safe is required outside localhost.");
  }
  const safe = address(safeInput ?? operatorAddress, "safe");
  if (!local && !hasFlag("allow-eoa-safe")) {
    await requireContract(ethers.provider, safe, "safe");
  }

  const requestedFile = configPath(false);
  const requestedFileExists = fs.existsSync(requestedFile);
  const file = requestedFileExists
    ? nextAvailablePath(requestedFile)
    : requestedFile;
  const existingConfig = requestedFileExists
    ? readJson<SaleConfigFile>(requestedFile)
    : undefined;
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
    ? saleNameFromConfigPath(file)
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
  });
  config.auction.fundsRecipient = config.lbp.strategy;
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

  writeJson(file, config);
  const infra = {
    chainId,
    safe,
    saleDeployer,
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
  const infraFile = path.join(saleDir, `${config.name}.infra.json`);
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
${requestedFileExists ? `  forkedFrom:                  ${requestedFile}\n` : ""}

Review the config schedule and economics, then run --action plan.
`);
}
