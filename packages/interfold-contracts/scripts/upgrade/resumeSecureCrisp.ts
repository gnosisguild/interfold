// SPDX-License-Identifier: LGPL-3.0-only
import path from "node:path";
import { pathToFileURL } from "node:url";

import { connect, hasFlag } from "../protocol/cli";
import {
  deploymentPath,
  governanceSafeBuilderPath,
  protocolDir,
  readJson,
  writeJson,
} from "../protocol/files";
import {
  aragonAdminSafeBatch,
  aragonAdminSafeTransactions,
  governanceBatch,
  proposeSafeBatch,
  safeTx,
} from "../protocol/safe";
import type {
  ProtocolDeployment,
  SecureCrispUpgradePlan,
} from "../protocol/types";
import { loadConfig } from "../protocol/values";
import { validateSecureCrispUpgrade } from "./validateSecureCrisp";

function planPath(name: string): string {
  return path.join(protocolDir, `${name}.secure-crisp.upgrade.json`);
}

function resumeBatchPath(name: string): string {
  return path.join(protocolDir, `${name}.secure-crisp.resume.safe.json`);
}

export async function prepareSecureCrispResume(): Promise<void> {
  if (!hasFlag("ciphernodes-restarted")) {
    throw new Error(
      "Restart every ciphernode on the planned release, confirm that the processes are online, and pass --ciphernodes-restarted",
    );
  }

  // This verifies the implementation, every BFV route and VK anchor, CRISP receipt bindings,
  // release policy, and paused and drained state before an unpause transaction can be created.
  await validateSecureCrispUpgrade();

  const { ethers } = await connect();
  const config = loadConfig();
  if (!config.governance) {
    throw new Error("Aragon governance is required for mainnet resume");
  }
  const deployment = readJson<ProtocolDeployment>(deploymentPath(config));
  const plan = readJson<SecureCrispUpgradePlan>(planPath(config.name));
  const interfold = await ethers.getContractAt(
    "Interfold",
    deployment.interfold,
  );
  const bonding = await ethers.getContractAt(
    "BondingRegistry",
    config.bondingRegistryProxy,
  );

  const requiredActive = config.interfold.committeeThresholds.reduce(
    (maximum, threshold) => {
      const total = BigInt(threshold.total);
      return total > maximum ? total : maximum;
    },
    0n,
  );
  const active = await bonding.numActiveOperators();
  if (active < requiredActive) {
    throw new Error(
      `Only ${active} release-ready operators are active; ${requiredActive} are required by the largest secure committee`,
    );
  }

  const txs = [
    safeTx(
      deployment.interfold,
      interfold.interface.encodeFunctionData("setRequestsPaused", [false]),
    ),
  ];
  const rawBatchFile = resumeBatchPath(config.name);
  const batch = governanceBatch(config, txs);
  batch.meta.name = `${config.name} secure CRISP resume`;
  batch.meta.description =
    "Resume E3 requests after secure CRISP validation and the ciphernode protocol cutover.";
  writeJson(rawBatchFile, batch);

  const safeBuilderFile = governanceSafeBuilderPath({
    ...config,
    name: `${config.name}.secure-crisp.resume`,
  });
  const safeBatch = aragonAdminSafeBatch(config, txs);
  safeBatch.meta.name = batch.meta.name;
  safeBatch.meta.description = batch.meta.description;
  writeJson(safeBuilderFile, safeBatch);

  if (hasFlag("propose-safe")) {
    await proposeSafeBatch(
      config,
      aragonAdminSafeTransactions(config, txs),
      config.governance.proposerSafe,
    );
  }

  console.log(`
Secure CRISP resume prepared
  node release:       ${plan.nodeRelease.version} (protocol ${plan.nodeRelease.protocolVersion})
  active operators:   ${active}/${requiredActive}
  governance batch:   ${rawBatchFile}
  Aragon Safe batch:  ${safeBuilderFile}
`);
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  prepareSecureCrispResume().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
