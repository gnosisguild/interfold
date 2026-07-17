// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

import { expect } from "chai";

import { ethers } from "../fixtures";

describe("ExitQueueLib", function () {
  it("counts a new tranche when a fully slashed tail has the same unlock timestamp", async function () {
    const [operator] = await ethers.getSigners();
    const harness = await ethers.deployContract("ExitQueueHarness");
    const operatorAddress = await operator.getAddress();

    await harness.queueSlashQueue(operatorAddress, 7 * 24 * 60 * 60, 10, 20);

    expect(await harness.liveTrancheCount(operatorAddress)).to.equal(1);
    expect(await harness.queueLength(operatorAddress)).to.equal(1);
    expect((await harness.tranche(operatorAddress, 0)).ticketAmount).to.equal(
      20,
    );
  });

  it("prunes drained tails and reuses both asset heads safely", async function () {
    const [operator] = await ethers.getSigners();
    const harness = await ethers.deployContract("ExitQueueHarness");
    const operatorAddress = await operator.getAddress();

    await harness.queue(operatorAddress, 0, 10, 20);
    await harness.claim(operatorAddress, 10, 20);
    expect(await harness.queueLength(operatorAddress)).to.equal(0);
    expect(await harness.liveTrancheCount(operatorAddress)).to.equal(0);

    await harness.queue(operatorAddress, 0, 30, 40);
    expect(await harness.claim.staticCall(operatorAddress, 30, 40)).to.deep.equal(
      [30n, 40n],
    );
    await harness.claim(operatorAddress, 30, 40);
    expect(await harness.queueLength(operatorAddress)).to.equal(0);
    expect(await harness.liveTrancheCount(operatorAddress)).to.equal(0);
  });
});
