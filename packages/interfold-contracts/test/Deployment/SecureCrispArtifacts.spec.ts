// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";
import fs from "node:fs";

import { repoRoot } from "../../scripts/protocol/files";
import { expectedCrispImageId } from "../../scripts/upgrade/secureCrispArtifacts";

describe("secure CRISP release artifacts", function () {
  it("reads the exact generated RISC Zero guest image", function () {
    const source = fs.readFileSync(
      `${repoRoot}/crates/support/contracts/ImageID.sol`,
      "utf8",
    );
    const imageId = expectedCrispImageId();

    expect(imageId).to.match(/^0x[0-9a-f]{64}$/);
    expect(source.toLowerCase()).to.include(`bytes32(${imageId})`);
  });
});
