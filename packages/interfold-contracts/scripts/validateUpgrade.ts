// SPDX-License-Identifier: LGPL-3.0-only
// Read-only storage-layout compatibility gate for upgradeable contracts.
// Missing baselines are fatal; maintainers create them explicitly with
// `pnpm snapshot:storage-layouts` from reviewed production build-info.
import * as fs from "fs";
import * as path from "path";

import {
  SNAPSHOT_DIR,
  type StandardCompilerInput,
  type StorageSnapshot,
  UPGRADEABLE_CONTRACTS,
  compilerInputPath,
  compilerInputSha256,
  diffLayouts,
  findCurrentLayout,
  sha256,
} from "./storageLayouts";

async function main(): Promise<void> {
  let totalErrors = 0;

  for (const { source, contract } of UPGRADEABLE_CONTRACTS) {
    const snapshotPath = path.join(SNAPSHOT_DIR, `${contract}.json`);
    if (!fs.existsSync(snapshotPath)) {
      console.error(
        `  ✗ ${contract}: required baseline is missing at ${snapshotPath}.`,
      );
      totalErrors += 1;
      continue;
    }

    const snapshot = JSON.parse(
      fs.readFileSync(snapshotPath, "utf8"),
    ) as StorageSnapshot;
    if (
      snapshot._format !== "interfold-storage-layout-v2" ||
      snapshot.contract !== contract ||
      snapshot.source !== source
    ) {
      console.error(`  ✗ ${contract}: baseline metadata is invalid.`);
      totalErrors += 1;
      continue;
    }

    const candidate = findCurrentLayout(source, contract);
    const errors = diffLayouts(
      contract,
      snapshot,
      candidate.layout,
      snapshot.metadata,
      candidate.metadata,
    );
    const inputPath = compilerInputPath(snapshot.baseline.compilerInputSha256);
    if (!fs.existsSync(inputPath)) {
      errors.push(`${contract}: archived compiler input is missing.`);
    } else {
      const archivedInput = JSON.parse(
        fs.readFileSync(inputPath, "utf8"),
      ) as StandardCompilerInput;
      if (
        compilerInputSha256(archivedInput) !==
        snapshot.baseline.compilerInputSha256
      ) {
        errors.push(`${contract}: archived compiler input hash is invalid.`);
      }
      const archivedSource =
        archivedInput.sources[source] ??
        archivedInput.sources[`project/${source}`];
      if (
        !archivedSource ||
        sha256(archivedSource.content) !== snapshot.baseline.sourceSha256
      ) {
        errors.push(
          `${contract}: archived compiler input does not contain the baseline source.`,
        );
      }
      if (
        archivedInput.settings.evmVersion !== snapshot.baseline.evmVersion ||
        (archivedInput.settings.optimizer?.runs ?? 0) !==
          snapshot.baseline.optimizerRuns
      ) {
        errors.push(
          `${contract}: archived compiler settings differ from the baseline metadata.`,
        );
      }
    }
    if (!/^[0-9a-f]{64}$/.test(snapshot.baseline.dependencyLockSha256)) {
      errors.push(`${contract}: dependency lock fingerprint is invalid.`);
    }
    if (
      candidate.compiler !== snapshot.baseline.compiler ||
      candidate.evmVersion !== snapshot.baseline.evmVersion ||
      candidate.optimizerRuns !== snapshot.baseline.optimizerRuns
    ) {
      errors.push(
        `${contract}: candidate compiler settings differ from the production baseline.`,
      );
    }
    if (errors.length === 0) {
      console.log(
        `  ✓ ${contract}: compatible with ${snapshot.baseline.sourceCommit} ` +
          `(${snapshot.baseline.buildInfoId}).`,
      );
    } else {
      totalErrors += errors.length;
      for (const error of errors) console.error(`  ✗ ${error}`);
    }
  }

  if (totalErrors > 0) {
    throw new Error(
      `validateUpgrade failed with ${totalErrors} storage-layout error${
        totalErrors === 1 ? "" : "s"
      }. Baselines are never created or modified by this command.`,
    );
  }

  console.log("validateUpgrade OK (read-only).");
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
