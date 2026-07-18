// SPDX-License-Identifier: LGPL-3.0-only
// Read-only storage-layout compatibility gate for upgradeable contracts.
// Missing baselines are fatal; maintainers create them explicitly with
// `pnpm snapshot:storage-layouts` from reviewed production build-info.
import * as fs from "fs";
import * as path from "path";

import {
  SNAPSHOT_DIR,
  type StorageSnapshot,
  UPGRADEABLE_CONTRACTS,
  diffLayouts,
  findCurrentLayout,
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
      snapshot._format !== "interfold-storage-layout-v1" ||
      snapshot.contract !== contract ||
      snapshot.source !== source
    ) {
      console.error(`  ✗ ${contract}: baseline metadata is invalid.`);
      totalErrors += 1;
      continue;
    }

    const candidate = findCurrentLayout(source, contract);
    const errors = diffLayouts(contract, snapshot, candidate.layout);
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
