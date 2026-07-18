// SPDX-License-Identifier: LGPL-3.0-only
// Explicit maintainer-only baseline generator. The caller must identify the
// reviewed production build-info and the exact source commit it represents.
import { execFileSync } from "child_process";
import * as fs from "fs";
import * as path from "path";

import {
  PACKAGE_DIR,
  SNAPSHOT_DIR,
  type StorageSnapshot,
  UPGRADEABLE_CONTRACTS,
  loadLayoutFromBuildInfo,
  sha256,
} from "./storageLayouts";

function argument(name: string): string {
  const index = process.argv.indexOf(name);
  const value = index === -1 ? undefined : process.argv[index + 1];
  if (!value || value.startsWith("--")) {
    throw new Error(`Missing required ${name} argument.`);
  }
  return value;
}

async function main(): Promise<void> {
  const outputPath = path.resolve(argument("--build-info"));
  const sourceCommit = argument("--source-commit");
  if (!/^[0-9a-f]{40}$/i.test(sourceCommit)) {
    throw new Error("--source-commit must be a full 40-character Git SHA.");
  }

  fs.mkdirSync(SNAPSHOT_DIR, { recursive: true });
  for (const { source, contract } of UPGRADEABLE_CONTRACTS) {
    const located = loadLayoutFromBuildInfo(outputPath, source, contract);
    const committedSource = execFileSync(
      "git",
      ["show", `${sourceCommit}:packages/interfold-contracts/${source}`],
      { cwd: PACKAGE_DIR, encoding: "utf8" },
    );
    if (sha256(committedSource) !== sha256(located.sourceContent)) {
      throw new Error(
        `${source}:${contract} in build-info does not match ${sourceCommit}.`,
      );
    }
    const snapshot: StorageSnapshot = {
      _format: "interfold-storage-layout-v1",
      contract,
      source,
      baseline: {
        buildInfoId: located.buildInfoId,
        compiler: located.compiler,
        evmVersion: located.evmVersion,
        optimizerRuns: located.optimizerRuns,
        sourceCommit,
        sourceSha256: sha256(located.sourceContent),
      },
      storage: located.layout.storage,
      types: located.layout.types,
    };
    const snapshotPath = path.join(SNAPSHOT_DIR, `${contract}.json`);
    fs.writeFileSync(snapshotPath, `${JSON.stringify(snapshot, null, 2)}\n`);
    console.log(`  * wrote ${snapshotPath}`);
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
