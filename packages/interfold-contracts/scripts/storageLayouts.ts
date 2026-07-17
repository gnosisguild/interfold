// SPDX-License-Identifier: LGPL-3.0-only
import { createHash } from "crypto";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

export interface StorageVar {
  astId: number;
  contract: string;
  label: string;
  offset: number;
  slot: string;
  type: string;
}

export interface StorageType {
  base?: string;
  encoding: string;
  key?: string;
  label: string;
  members?: StorageVar[];
  numberOfBytes: string;
  value?: string;
}

export interface StorageLayout {
  storage: StorageVar[];
  types: Record<string, StorageType>;
}

export interface StorageSnapshot extends StorageLayout {
  _format: "interfold-storage-layout-v1";
  baseline: {
    buildInfoId: string;
    compiler: string;
    evmVersion: string;
    optimizerRuns: number;
    sourceCommit: string;
    sourceSha256: string;
  };
  contract: string;
  source: string;
}

export const UPGRADEABLE_CONTRACTS = [
  { source: "contracts/Interfold.sol", contract: "Interfold" },
  {
    source: "contracts/registry/CiphernodeRegistryOwnable.sol",
    contract: "CiphernodeRegistryOwnable",
  },
  {
    source: "contracts/registry/BondingRegistry.sol",
    contract: "BondingRegistry",
  },
  {
    source: "contracts/E3RefundManager.sol",
    contract: "E3RefundManager",
  },
] as const;

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
export const PACKAGE_DIR = path.resolve(SCRIPT_DIR, "..");
export const SNAPSHOT_DIR = path.join(PACKAGE_DIR, "audits/storage-layouts");
export const BUILD_INFO_DIR = path.join(PACKAGE_DIR, "artifacts/build-info");

interface BuildInfoInput {
  id: string;
  input: {
    settings: {
      evmVersion?: string;
      optimizer?: { runs?: number };
    };
    sources: Record<string, { content: string }>;
  };
  solcLongVersion?: string;
  solcVersion: string;
}

interface BuildInfoOutput {
  output: {
    contracts: Record<
      string,
      Record<string, { storageLayout?: StorageLayout }>
    >;
  };
}

export interface LocatedLayout {
  buildInfoId: string;
  compiler: string;
  evmVersion: string;
  layout: StorageLayout;
  optimizerRuns: number;
  sourceContent: string;
  sourceKey: string;
}

function pairedInputPath(outputPath: string): string {
  if (!outputPath.endsWith(".output.json")) {
    throw new Error(`Expected a *.output.json build-info path: ${outputPath}`);
  }
  return outputPath.replace(/\.output\.json$/, ".json");
}

function sourceKeys(source: string): string[] {
  return [source, `project/${source}`];
}

export function loadLayoutFromBuildInfo(
  outputPath: string,
  source: string,
  contract: string,
): LocatedLayout {
  const inputPath = pairedInputPath(outputPath);
  if (!fs.existsSync(inputPath)) {
    throw new Error(`Paired build-info input is missing: ${inputPath}`);
  }

  const input = JSON.parse(
    fs.readFileSync(inputPath, "utf8"),
  ) as BuildInfoInput;
  const output = JSON.parse(
    fs.readFileSync(outputPath, "utf8"),
  ) as BuildInfoOutput;

  for (const sourceKey of sourceKeys(source)) {
    const layout =
      output.output.contracts?.[sourceKey]?.[contract]?.storageLayout;
    const sourceContent = input.input.sources?.[sourceKey]?.content;
    if (layout && sourceContent !== undefined) {
      return {
        buildInfoId: input.id,
        compiler: input.solcLongVersion ?? input.solcVersion,
        evmVersion: input.input.settings.evmVersion ?? "unknown",
        layout,
        optimizerRuns: input.input.settings.optimizer?.runs ?? 0,
        sourceContent,
        sourceKey,
      };
    }
  }

  throw new Error(`${source}:${contract} is not present in ${outputPath}.`);
}

export function findCurrentLayout(
  source: string,
  contract: string,
): LocatedLayout {
  if (!fs.existsSync(BUILD_INFO_DIR)) {
    throw new Error(`No build-info directory. Run Hardhat compile first.`);
  }

  const currentSource = fs.readFileSync(path.join(PACKAGE_DIR, source), "utf8");
  const outputs = fs
    .readdirSync(BUILD_INFO_DIR)
    .filter((name) => name.endsWith(".output.json"))
    .map((name) => path.join(BUILD_INFO_DIR, name))
    .sort(
      (left, right) => fs.statSync(right).mtimeMs - fs.statSync(left).mtimeMs,
    );

  for (const outputPath of outputs) {
    try {
      const located = loadLayoutFromBuildInfo(outputPath, source, contract);
      if (located.sourceContent === currentSource) return located;
    } catch {
      // Incremental Hardhat build-info files contain only their compilation job.
    }
  }

  throw new Error(
    `No build-info storage layout matches the current ${source}:${contract}. ` +
      `Run \`pnpm compile:contracts --force\` and retry.`,
  );
}

export function sha256(content: string): string {
  return createHash("sha256").update(content).digest("hex");
}

function compareType(
  contract: string,
  pathLabel: string,
  previous: StorageLayout,
  previousTypeId: string,
  current: StorageLayout,
  currentTypeId: string,
  errors: string[],
  visited: Set<string>,
): void {
  const visitKey = `${previousTypeId}:${currentTypeId}`;
  if (visited.has(visitKey)) return;
  visited.add(visitKey);

  const before = previous.types[previousTypeId];
  const after = current.types[currentTypeId];
  if (!before || !after) {
    errors.push(
      `${contract}: ${pathLabel} has unresolved storage type metadata.`,
    );
    return;
  }
  if (before.encoding !== after.encoding || before.label !== after.label) {
    errors.push(
      `${contract}: ${pathLabel} type changed from ${before.label}/${before.encoding} ` +
        `to ${after.label}/${after.encoding}.`,
    );
    return;
  }

  for (const key of ["base", "key", "value"] as const) {
    const beforeChild = before[key];
    const afterChild = after[key];
    if ((beforeChild === undefined) !== (afterChild === undefined)) {
      errors.push(`${contract}: ${pathLabel} ${key} type changed.`);
    } else if (beforeChild && afterChild) {
      compareType(
        contract,
        `${pathLabel}.${key}`,
        previous,
        beforeChild,
        current,
        afterChild,
        errors,
        visited,
      );
    }
  }

  if (before.members) {
    if (!after.members) {
      errors.push(
        `${contract}: ${pathLabel} no longer exposes struct members.`,
      );
      return;
    }
    for (const oldMember of before.members) {
      const newMember = after.members.find(
        (candidate) => candidate.label === oldMember.label,
      );
      if (!newMember) {
        errors.push(
          `${contract}: ${pathLabel}.${oldMember.label} was removed or renamed.`,
        );
        continue;
      }
      if (
        oldMember.slot !== newMember.slot ||
        oldMember.offset !== newMember.offset
      ) {
        errors.push(
          `${contract}: ${pathLabel}.${oldMember.label} moved from ` +
            `${oldMember.slot}+${oldMember.offset} to ` +
            `${newMember.slot}+${newMember.offset}.`,
        );
      }
      compareType(
        contract,
        `${pathLabel}.${oldMember.label}`,
        previous,
        oldMember.type,
        current,
        newMember.type,
        errors,
        visited,
      );
    }
  } else if (before.numberOfBytes !== after.numberOfBytes) {
    errors.push(
      `${contract}: ${pathLabel} size changed from ${before.numberOfBytes} ` +
        `to ${after.numberOfBytes} bytes.`,
    );
  }
}

function gapEnd(layout: StorageLayout, gap: StorageVar): bigint {
  const bytes = BigInt(layout.types[gap.type].numberOfBytes);
  return BigInt(gap.slot) + bytes / 32n;
}

export function diffLayouts(
  contract: string,
  previous: StorageLayout,
  current: StorageLayout,
): string[] {
  const errors: string[] = [];
  const previousGap = previous.storage.find((entry) => entry.label === "__gap");
  const currentGap = current.storage.find((entry) => entry.label === "__gap");

  for (const oldEntry of previous.storage) {
    if (oldEntry.label === "__gap") continue;
    const newEntry = current.storage.find(
      (candidate) => candidate.label === oldEntry.label,
    );
    if (!newEntry) {
      errors.push(
        `${contract}: state variable \`${oldEntry.label}\` was removed or renamed.`,
      );
      continue;
    }
    if (
      oldEntry.slot !== newEntry.slot ||
      oldEntry.offset !== newEntry.offset
    ) {
      errors.push(
        `${contract}: \`${oldEntry.label}\` moved from ${oldEntry.slot}+${oldEntry.offset} ` +
          `to ${newEntry.slot}+${newEntry.offset}.`,
      );
    }
    compareType(
      contract,
      oldEntry.label,
      previous,
      oldEntry.type,
      current,
      newEntry.type,
      errors,
      new Set(),
    );
  }

  if (previousGap) {
    if (!currentGap) {
      errors.push(`${contract}: reserved __gap was removed.`);
      return errors;
    }
    if (gapEnd(previous, previousGap) !== gapEnd(current, currentGap)) {
      errors.push(
        `${contract}: reserved __gap must shrink from the front without changing its end slot.`,
      );
    }
    const oldGapStart = BigInt(previousGap.slot);
    const newGapStart = BigInt(currentGap.slot);
    if (newGapStart < oldGapStart) {
      errors.push(`${contract}: reserved __gap moved backward.`);
    }
    const previousLabels = new Set(
      previous.storage.map((entry) => entry.label),
    );
    for (const entry of current.storage) {
      if (previousLabels.has(entry.label) || entry.label === "__gap") continue;
      const slot = BigInt(entry.slot);
      if (slot < oldGapStart || slot >= newGapStart) {
        errors.push(
          `${contract}: new variable \`${entry.label}\` at slot ${entry.slot} ` +
            `does not consume the front of the reserved gap.`,
        );
      }
    }
  }

  return errors;
}
