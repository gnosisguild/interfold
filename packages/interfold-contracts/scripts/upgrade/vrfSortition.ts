// SPDX-License-Identifier: LGPL-3.0-only
import path from "node:path";

import { connect, hasFlag } from "../protocol/cli";
import { proxyAdminInterface } from "../protocol/constants";
import {
  deploymentPath,
  governanceSafeBuilderPath,
  protocolDir,
  readJson,
  writeJson,
} from "../protocol/files";
import {
  assertVrfSubscription,
  buildRandomnessTransactions,
  deployRandomnessProvider,
} from "../protocol/randomness";
import {
  aragonAdminSafeBatch,
  aragonAdminSafeTransactions,
  governanceBatch,
  proposeSafeBatch,
  safeTx,
} from "../protocol/safe";
import type {
  ProtocolConfigFile,
  ProtocolDeployment,
  SafeTransaction,
  VrfSortitionUpgradePlan,
} from "../protocol/types";
import { loadConfig, requireContract } from "../protocol/values";
import { deployUpgradeImplementation } from "./safeProxyUpgrade";

function planPath(config: ProtocolConfigFile): string {
  return path.join(protocolDir, `${config.name}.vrf-sortition.upgrade.json`);
}

function batchPath(config: ProtocolConfigFile): string {
  return path.join(
    protocolDir,
    `${config.name}.vrf-sortition.upgrade.safe.json`,
  );
}

async function requireProxyAdminOwner(
  ethers: any,
  proxyAdmin: string,
  expectedOwner: string,
  label: string,
): Promise<void> {
  await requireContract(ethers.provider, proxyAdmin, label);
  const admin = await ethers.getContractAt("ProxyAdmin", proxyAdmin);
  const actualOwner = await admin.owner();
  if (actualOwner.toLowerCase() !== expectedOwner.toLowerCase()) {
    throw new Error(
      `${label} owner mismatch: expected ${expectedOwner}, got ${actualOwner}`,
    );
  }
}

function upgradeTransaction(
  proxyAdmin: string,
  proxy: string,
  implementation: string,
): SafeTransaction {
  return safeTx(
    proxyAdmin,
    proxyAdminInterface.encodeFunctionData("upgradeAndCall", [
      proxy,
      implementation,
      "0x",
    ]),
  );
}

export async function prepareVrfSortitionUpgrade(): Promise<void> {
  const { ethers } = await connect();
  const config = loadConfig();
  const deployment = readJson<ProtocolDeployment>(deploymentPath(config));
  const network = await ethers.provider.getNetwork();
  if (Number(network.chainId) !== deployment.chainId) {
    throw new Error("Connected to the wrong network for this deployment file");
  }

  await Promise.all([
    requireContract(
      ethers.provider,
      deployment.ciphernodeRegistry,
      "ciphernode registry proxy",
    ),
    requireContract(ethers.provider, deployment.interfold, "Interfold proxy"),
    requireProxyAdminOwner(
      ethers,
      deployment.ciphernodeRegistryProxyAdmin,
      config.protocolOwner,
      "ciphernode registry ProxyAdmin",
    ),
    requireProxyAdminOwner(
      ethers,
      deployment.interfoldProxyAdmin,
      config.protocolOwner,
      "Interfold ProxyAdmin",
    ),
    assertVrfSubscription(ethers, config),
  ]);

  const interfold = await ethers.getContractAt(
    "Interfold",
    deployment.interfold,
  );
  const registry = await ethers.getContractAt(
    "CiphernodeRegistryOwnable",
    deployment.ciphernodeRegistry,
  );
  if (!(await interfold.requestsPaused())) {
    throw new Error("Pause E3 requests before preparing the VRF upgrade");
  }
  const activeE3Count = await interfold.activeE3Count();
  if (activeE3Count !== 0n) {
    throw new Error(`Interfold still has ${activeE3Count} active E3s`);
  }
  const unreleasedCommittees = await registry.unreleasedCommitteeCount();
  if (unreleasedCommittees !== 0n) {
    throw new Error(
      `CiphernodeRegistry still has ${unreleasedCommittees} unreleased committees`,
    );
  }

  const [operator] = await ethers.getSigners();
  const registryUpgrade = await deployUpgradeImplementation(
    ethers,
    operator,
    "ciphernodeRegistry",
    deployment,
  );
  const interfoldUpgrade = await deployUpgradeImplementation(
    ethers,
    operator,
    "interfold",
    deployment,
  );
  if (!registryUpgrade.sortitionLibrary) {
    throw new Error("Registry sortition library was not deployed");
  }
  if (!interfoldUpgrade.lifecycleLibrary || !interfoldUpgrade.pricingLibrary) {
    throw new Error("Interfold libraries were not deployed");
  }
  const randomness = await deployRandomnessProvider(
    ethers,
    operator,
    config,
    deployment.ciphernodeRegistry,
  );

  const txs: SafeTransaction[] = [
    upgradeTransaction(
      deployment.ciphernodeRegistryProxyAdmin,
      deployment.ciphernodeRegistry,
      registryUpgrade.implementation,
    ),
    upgradeTransaction(
      deployment.interfoldProxyAdmin,
      deployment.interfold,
      interfoldUpgrade.implementation,
    ),
    ...buildRandomnessTransactions(
      config,
      randomness.randomnessProvider,
      deployment.ciphernodeRegistry,
      registry.interface,
      randomness.randomnessProviderOwnershipAcceptanceRequired,
    ),
  ];

  const rawBatchFile = batchPath(config);
  const batch = governanceBatch(config, txs);
  batch.meta.name = `${config.name} VRF sortition upgrade`;
  batch.meta.description =
    "Upgrade committee sortition to Chainlink VRF and configure its subscription consumer.";
  writeJson(rawBatchFile, batch);

  let safeBuilderFile: string | undefined;
  if (config.governance) {
    safeBuilderFile = governanceSafeBuilderPath({
      ...config,
      name: `${config.name}.vrf-sortition.upgrade`,
    });
    const safeBatch = aragonAdminSafeBatch(config, txs);
    safeBatch.meta.name = `${config.name} VRF sortition upgrade`;
    writeJson(safeBuilderFile, safeBatch);
  }

  const plan: VrfSortitionUpgradePlan = {
    name: config.name,
    operator: await operator.getAddress(),
    protocolOwner: config.protocolOwner,
    registryProxy: deployment.ciphernodeRegistry,
    registryProxyAdmin: deployment.ciphernodeRegistryProxyAdmin,
    registryImplementation: registryUpgrade.implementation,
    sortitionLibrary: registryUpgrade.sortitionLibrary,
    interfoldProxy: deployment.interfold,
    interfoldProxyAdmin: deployment.interfoldProxyAdmin,
    interfoldImplementation: interfoldUpgrade.implementation,
    lifecycleLibrary: interfoldUpgrade.lifecycleLibrary,
    pricingLibrary: interfoldUpgrade.pricingLibrary,
    randomnessProvider: randomness.randomnessProvider,
    randomnessProviderOwnershipAcceptanceRequired:
      randomness.randomnessProviderOwnershipAcceptanceRequired,
    safeTransactions: rawBatchFile,
    governanceSafeBuilder: safeBuilderFile,
  };

  if (hasFlag("propose-safe")) {
    const proposalTransactions = config.governance
      ? aragonAdminSafeTransactions(config, txs)
      : txs;
    plan.safeProposal = await proposeSafeBatch(
      config,
      proposalTransactions,
      config.governance?.proposerSafe ?? config.safe,
    );
  }
  writeJson(planPath(config), plan);

  console.log(`
VRF sortition upgrade prepared
  registry implementation: ${plan.registryImplementation}
  Interfold implementation:${plan.interfoldImplementation}
  randomness provider:      ${plan.randomnessProvider}
  governance batch:         ${plan.safeTransactions}
  Aragon Safe batch:        ${plan.governanceSafeBuilder ?? "(not configured)"}
  transactions:             ${txs.length}
  requests remain paused after execution
`);
}

prepareVrfSortitionUpgrade().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
