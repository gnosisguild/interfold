// SPDX-License-Identifier: LGPL-3.0-only
import path from "node:path";

import { connect } from "../protocol/cli";
import {
  deploymentPath,
  protocolDir,
  readJson,
  writeJson,
} from "../protocol/files";
import {
  assertVrfSubscription,
  assertVrfUpgradePlanMatchesDeployment,
  requirePlannedRandomnessConfig,
} from "../protocol/randomness";
import type {
  ProtocolDeployment,
  VrfSortitionUpgradePlan,
} from "../protocol/types";
import { loadConfig, requireContract } from "../protocol/values";
import { proxyImplementation } from "./safeProxyUpgrade";

function assertEqual(label: string, actual: unknown, expected: unknown): void {
  if (String(actual).toLowerCase() !== String(expected).toLowerCase()) {
    throw new Error(`${label}: expected ${expected}, got ${actual}`);
  }
  console.log(`  ok ${label}`);
}

export async function validateVrfSortitionUpgrade(): Promise<void> {
  const { ethers } = await connect();
  const config = loadConfig();
  const deploymentFile = deploymentPath(config);
  const deployment = readJson<ProtocolDeployment>(deploymentFile);
  const plan = readJson<VrfSortitionUpgradePlan>(
    path.join(protocolDir, `${config.name}.vrf-sortition.upgrade.json`),
  );
  const network = await ethers.provider.getNetwork();
  assertVrfUpgradePlanMatchesDeployment(
    config,
    deployment,
    plan,
    network.chainId,
  );
  const randomness = requirePlannedRandomnessConfig(config, plan);
  const effectiveConfig = { ...config, randomness };

  for (const [label, target] of [
    ["registry implementation", plan.registryImplementation],
    ["sortition library", plan.sortitionLibrary],
    ["Interfold implementation", plan.interfoldImplementation],
    ["lifecycle library", plan.lifecycleLibrary],
    ["pricing library", plan.pricingLibrary],
    ["randomness provider", plan.randomnessProvider],
  ] as const) {
    await requireContract(ethers.provider, target, label);
  }

  assertEqual(
    "registry implementation",
    await proxyImplementation(ethers, plan.registryProxy),
    plan.registryImplementation,
  );
  assertEqual(
    "Interfold implementation",
    await proxyImplementation(ethers, plan.interfoldProxy),
    plan.interfoldImplementation,
  );

  const registry = await ethers.getContractAt(
    "CiphernodeRegistryOwnable",
    plan.registryProxy,
  );
  const interfold = await ethers.getContractAt(
    "Interfold",
    plan.interfoldProxy,
  );
  const provider = await ethers.getContractAt(
    "ChainlinkVrfRandomnessProvider",
    plan.randomnessProvider,
  );
  for (const [label, actual, expected] of [
    [
      "registry.randomnessProvider",
      await registry.randomnessProvider(),
      plan.randomnessProvider,
    ],
    [
      "registry.randomnessRequestTimeout",
      await registry.randomnessRequestTimeout(),
      randomness.requestTimeout,
    ],
    ["provider.owner", await provider.owner(), config.protocolOwner],
    ["provider.requester", await provider.requester(), plan.registryProxy],
    [
      "provider.coordinator",
      await provider.s_vrfCoordinator(),
      randomness.coordinator,
    ],
    [
      "provider.subscriptionId",
      await provider.subscriptionId(),
      randomness.subscriptionId,
    ],
    ["provider.keyHash", await provider.keyHash(), randomness.keyHash],
    [
      "provider.requestConfirmations",
      await provider.requestConfirmations(),
      randomness.requestConfirmations,
    ],
    [
      "provider.callbackGasLimit",
      await provider.callbackGasLimit(),
      randomness.callbackGasLimit,
    ],
    [
      "provider.nativePayment",
      await provider.nativePayment(),
      randomness.nativePayment,
    ],
    [
      "provider.minimumSubscriptionBalance",
      await provider.minimumSubscriptionBalance(),
      randomness.minimumSubscriptionBalance,
    ],
  ] as const) {
    assertEqual(label, actual, expected);
  }

  if (!(await interfold.requestsPaused())) {
    throw new Error("Interfold requests must remain paused during validation");
  }
  assertEqual("interfold.activeE3Count", await interfold.activeE3Count(), 0);
  assertEqual(
    "interfold.pricing.randomnessFlatFee",
    (await interfold.getPricingConfig()).randomnessFlatFee,
    BigInt(plan.randomnessFlatFee),
  );
  assertEqual(
    "registry.unreleasedCommitteeCount",
    await registry.unreleasedCommitteeCount(),
    0,
  );

  await assertVrfSubscription(ethers, effectiveConfig, plan.randomnessProvider);
  console.log("  ok VRF subscription consumer");

  writeJson(deploymentFile, {
    ...deployment,
    registrySortitionLib: plan.sortitionLibrary,
    ciphernodeRegistryImplementation: plan.registryImplementation,
    interfoldImplementation: plan.interfoldImplementation,
    interfoldLifecycle: plan.lifecycleLibrary,
    interfoldPricing: plan.pricingLibrary,
    randomnessProvider: plan.randomnessProvider,
    randomness,
    randomnessProviderOwnershipAcceptanceRequired: false,
  });
  console.log(`VRF sortition upgrade validated; updated ${deploymentFile}`);
  console.log(
    "E3 requests remain paused. Restart every ciphernode before preparing the resume transaction.",
  );
}

validateVrfSortitionUpgrade().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
