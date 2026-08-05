// SPDX-License-Identifier: LGPL-3.0-only
import path from "path";

import { CiphernodeRegistryOwnable__factory as RegistryFactory } from "../../types";
import { connect, hasFlag } from "../protocol/cli";
import { proxyAdminInterface } from "../protocol/constants";
import {
  deploymentPath,
  protocolDir,
  readJson,
  writeJson,
} from "../protocol/files";
import { proposeSafeBatch, safeBatch, safeTx } from "../protocol/safe";
import type {
  ProtocolConfigFile,
  ProtocolDeployment,
  SafeProposal,
  SafeTransaction,
} from "../protocol/types";
import {
  deployedAddress,
  loadConfig,
  requireContract,
} from "../protocol/values";

export type UpgradeTarget =
  | "bondingRegistry"
  | "ciphernodeRegistry"
  | "interfold"
  | "e3RefundManager";

interface UpgradePlan {
  name: string;
  target: UpgradeTarget;
  proxy: string;
  proxyAdmin: string;
  implementation: string;
  assetLibrary?: string;
  eligibilityLibrary?: string;
  slashingLibrary?: string;
  lifecycleLibrary?: string;
  pricingLibrary?: string;
  operator: string;
  safe: string;
  safeTransactions: string;
  safeProposal?: SafeProposal;
}

export async function proposeProxyUpgrade(
  target: UpgradeTarget,
): Promise<void> {
  const { ethers } = await connect();
  const config = loadConfig();
  const deployment = readJson<ProtocolDeployment>(deploymentPath(config));
  const network = await ethers.provider.getNetwork();
  if (Number(network.chainId) !== deployment.chainId) {
    throw new Error("Connected to the wrong network for this deployment file");
  }

  const [operator] = await ethers.getSigners();
  const operatorAddress = await operator.getAddress();
  const deployed = await deployImplementation(
    ethers,
    operator,
    target,
    deployment,
  );
  const proxy = proxyFor(target, config, deployment);
  const proxyAdmin = proxyAdminFor(target, config, deployment);

  await requireContract(ethers.provider, proxy, `${target} proxy`);
  await requireContract(ethers.provider, proxyAdmin, `${target} ProxyAdmin`);
  const admin = await ethers.getContractAt("ProxyAdmin", proxyAdmin);
  const adminOwner = await admin.owner();
  if (adminOwner.toLowerCase() !== config.safe.toLowerCase()) {
    throw new Error(
      `${target} ProxyAdmin owner mismatch: expected ${config.safe}, got ${adminOwner}`,
    );
  }

  const txs = [
    safeTx(
      proxyAdmin,
      proxyAdminInterface.encodeFunctionData("upgradeAndCall", [
        proxy,
        deployed.implementation,
        "0x",
      ]),
    ),
  ];
  const batchFile = upgradeBatchPath(config, target);
  const batch = safeBatch(config, txs);
  batch.meta.name = `${config.name} ${target} upgrade`;
  batch.meta.description = `Upgrade ${target} implementation through its Safe-owned ProxyAdmin.`;
  writeJson(batchFile, batch);

  const plan: UpgradePlan = {
    name: config.name,
    target,
    proxy,
    proxyAdmin,
    implementation: deployed.implementation,
    assetLibrary: deployed.assetLibrary,
    eligibilityLibrary: deployed.eligibilityLibrary,
    slashingLibrary: deployed.slashingLibrary,
    lifecycleLibrary: deployed.lifecycleLibrary,
    pricingLibrary: deployed.pricingLibrary,
    operator: operatorAddress,
    safe: config.safe,
    safeTransactions: batchFile,
  };

  if (hasFlag("propose-safe")) {
    plan.safeProposal = await proposeSafeBatch(config, txs);
  }
  writeJson(upgradePlanPath(config, target), plan);

  printPlan(plan, txs);
}

async function deployImplementation(
  ethers: any,
  operator: any,
  target: UpgradeTarget,
  deployment: ProtocolDeployment,
): Promise<{
  implementation: string;
  assetLibrary?: string;
  eligibilityLibrary?: string;
  slashingLibrary?: string;
  lifecycleLibrary?: string;
  pricingLibrary?: string;
}> {
  if (target === "interfold") {
    const pricingFactory = await ethers.getContractFactory("InterfoldPricing");
    const pricing = await pricingFactory.deploy();
    await pricing.waitForDeployment();
    const pricingLibrary = await deployedAddress(pricing);

    const lifecycleFactory =
      await ethers.getContractFactory("InterfoldLifecycle");
    const lifecycle = await lifecycleFactory.deploy();
    await lifecycle.waitForDeployment();
    const lifecycleLibrary = await deployedAddress(lifecycle);

    const factory = await ethers.getContractFactory("Interfold", {
      libraries: {
        InterfoldLifecycle: lifecycleLibrary,
        InterfoldPricing: pricingLibrary,
      },
    });
    const implementation = await factory.deploy();
    await implementation.waitForDeployment();
    return {
      implementation: await deployedAddress(implementation),
      lifecycleLibrary,
      pricingLibrary,
    };
  }

  if (target === "ciphernodeRegistry") {
    const factory = await ethers.getContractFactory(
      RegistryFactory.abi,
      RegistryFactory.linkBytecode({
        "npm/poseidon-solidity@0.0.5/PoseidonT3.sol:PoseidonT3":
          deployment.poseidonT3,
      }),
      operator,
    );
    const implementation = await factory.deploy();
    await implementation.waitForDeployment();
    return { implementation: await deployedAddress(implementation) };
  }

  if (target === "bondingRegistry") {
    const assetFactory = await ethers.getContractFactory("BondingAssetLib");
    const asset = await assetFactory.deploy();
    await asset.waitForDeployment();
    const assetLibrary = await deployedAddress(asset);

    const eligibilityFactory = await ethers.getContractFactory(
      "BondingEligibilityLib",
    );
    const eligibility = await eligibilityFactory.deploy();
    await eligibility.waitForDeployment();
    const eligibilityLibrary = await deployedAddress(eligibility);

    const slashingFactory =
      await ethers.getContractFactory("BondingSlashingLib");
    const slashing = await slashingFactory.deploy();
    await slashing.waitForDeployment();
    const slashingLibrary = await deployedAddress(slashing);

    const factory = await ethers.getContractFactory("BondingRegistry", {
      libraries: {
        BondingAssetLib: assetLibrary,
        BondingEligibilityLib: eligibilityLibrary,
        BondingSlashingLib: slashingLibrary,
      },
    });
    const implementation = await factory.deploy();
    await implementation.waitForDeployment();
    return {
      implementation: await deployedAddress(implementation),
      assetLibrary,
      eligibilityLibrary,
      slashingLibrary,
    };
  }

  const factory = await ethers.getContractFactory("E3RefundManager");
  const implementation = await factory.deploy();
  await implementation.waitForDeployment();
  return { implementation: await deployedAddress(implementation) };
}

function proxyFor(
  target: UpgradeTarget,
  config: ProtocolConfigFile,
  deployment: ProtocolDeployment,
): string {
  if (target === "bondingRegistry") return config.bondingRegistryProxy;
  if (target === "ciphernodeRegistry") return deployment.ciphernodeRegistry;
  if (target === "interfold") return deployment.interfold;
  return deployment.e3RefundManager;
}

function proxyAdminFor(
  target: UpgradeTarget,
  config: ProtocolConfigFile,
  deployment: ProtocolDeployment,
): string {
  if (target === "bondingRegistry") return config.bondingRegistryProxyAdmin;
  if (target === "ciphernodeRegistry")
    return deployment.ciphernodeRegistryProxyAdmin;
  if (target === "interfold") return deployment.interfoldProxyAdmin;
  return deployment.e3RefundManagerProxyAdmin;
}

function upgradeBatchPath(
  config: ProtocolConfigFile,
  target: UpgradeTarget,
): string {
  return path.join(protocolDir, `${config.name}.${target}.upgrade.safe.json`);
}

function upgradePlanPath(
  config: ProtocolConfigFile,
  target: UpgradeTarget,
): string {
  return path.join(protocolDir, `${config.name}.${target}.upgrade.json`);
}

function printPlan(plan: UpgradePlan, txs: SafeTransaction[]): void {
  console.log(`
Protocol upgrade prepared
  target:          ${plan.target}
  proxy:           ${plan.proxy}
  proxyAdmin:      ${plan.proxyAdmin}
  implementation:  ${plan.implementation}
  assetLibrary:    ${plan.assetLibrary ?? "(not applicable)"}
  eligibilityLibrary: ${plan.eligibilityLibrary ?? "(not applicable)"}
  slashingLibrary: ${plan.slashingLibrary ?? "(not applicable)"}
  lifecycleLibrary: ${plan.lifecycleLibrary ?? "(not applicable)"}
  pricingLibrary:  ${plan.pricingLibrary ?? "(not applicable)"}
  operator:        ${plan.operator}
  Safe owner:      ${plan.safe}
  Safe batch:      ${plan.safeTransactions}
  txs:             ${txs.length}
  proposal:        ${plan.safeProposal?.url ?? "(not proposed)"}
`);
}
