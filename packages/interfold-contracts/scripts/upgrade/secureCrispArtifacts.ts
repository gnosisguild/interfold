// SPDX-License-Identifier: LGPL-3.0-only
import fs from "node:fs";
import path from "node:path";

import { repoRoot } from "../protocol/files";

/** Read the RISC Zero guest image that this source tree built and deploys. */
export function expectedCrispImageId(): string {
  const imageFile = path.join(
    repoRoot,
    "crates",
    "support",
    "contracts",
    "ImageID.sol",
  );
  if (!fs.existsSync(imageFile)) {
    throw new Error(`CRISP image ID file not found: ${imageFile}`);
  }
  const source = fs.readFileSync(imageFile, "utf8");
  const match = source.match(
    /bytes32 public constant PROGRAM_ID = bytes32\((0x[0-9a-fA-F]{64})\)/,
  );
  if (!match) {
    throw new Error(`CRISP PROGRAM_ID not found in ${imageFile}`);
  }
  return match[1].toLowerCase();
}
