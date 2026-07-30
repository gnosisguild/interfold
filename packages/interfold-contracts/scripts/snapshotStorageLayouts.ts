// SPDX-License-Identifier: LGPL-3.0-only
// Explicit maintainer-only baseline generator. The caller must identify the
// reviewed production build-info and the exact source commit it represents.
import { execFileSync } from "child_process";
import * as fs from "fs";
import * as path from "path";

import {
  COMPILER_INPUT_DIR,
  SNAPSHOT_DIR,
  type StorageSnapshot,
  UPGRADEABLE_CONTRACTS,
  WORKSPACE_DIR,
  compilerInputPath,
  compilerInputSha256,
  loadLayoutFromBuildInfo,
  pnpmPackageKeys,
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

  const committedLock = execFileSync(
    "git",
    ["show", `${sourceCommit}:pnpm-lock.yaml`],
    { cwd: WORKSPACE_DIR, encoding: "utf8" },
  );
  const committedPackages = pnpmPackageKeys(committedLock);
  fs.mkdirSync(SNAPSHOT_DIR, { recursive: true });
  fs.mkdirSync(COMPILER_INPUT_DIR, { recursive: true });
  for (const { source, contract } of UPGRADEABLE_CONTRACTS) {
    const located = loadLayoutFromBuildInfo(outputPath, source, contract);
    for (const [sourceKey, compilerSource] of Object.entries(
      located.compilerInput.sources,
    )) {
      if (sourceKey.startsWith("project/")) {
        const expectedSource = execFileSync(
          "git",
          [
            "show",
            `${sourceCommit}:packages/interfold-contracts/${sourceKey.slice(
              "project/".length,
            )}`,
          ],
          { cwd: WORKSPACE_DIR, encoding: "utf8" },
        );
        if (sha256(expectedSource) !== sha256(compilerSource.content)) {
          throw new Error(
            `${sourceKey} in build-info does not match ${sourceCommit}.`,
          );
        }
        continue;
      }

      const npmMatch = sourceKey.match(
        /^npm\/(@[^/]+\/[^@/]+|[^/@]+)@([^/]+)\//,
      );
      if (
        !npmMatch?.[1] ||
        !npmMatch[2] ||
        !committedPackages.has(`${npmMatch[1]}@${npmMatch[2]}`)
      ) {
        throw new Error(
          `${sourceKey} is not pinned by ${sourceCommit}:pnpm-lock.yaml.`,
        );
      }
    }

    const inputSha256 = compilerInputSha256(located.compilerInput);
    const inputPath = compilerInputPath(inputSha256);
    const archivedInput = `${JSON.stringify(located.compilerInput, null, 2)}\n`;
    if (
      fs.existsSync(inputPath) &&
      compilerInputSha256(
        JSON.parse(
          fs.readFileSync(inputPath, "utf8"),
        ) as typeof located.compilerInput,
      ) !== inputSha256
    ) {
      throw new Error(`Archived compiler input is invalid: ${inputPath}.`);
    }
    if (!fs.existsSync(inputPath)) fs.writeFileSync(inputPath, archivedInput);

    const snapshot: StorageSnapshot = {
      _format: "interfold-storage-layout-v2",
      contract,
      source,
      baseline: {
        buildInfoId: located.buildInfoId,
        compiler: located.compiler,
        compilerInputSha256: inputSha256,
        dependencyLockSha256: sha256(committedLock),
        evmVersion: located.evmVersion,
        optimizerRuns: located.optimizerRuns,
        sourceCommit,
        sourceSha256: sha256(located.sourceContent),
      },
      metadata: located.metadata,
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
