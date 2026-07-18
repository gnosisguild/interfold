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
    expect(
      await harness.claim.staticCall(operatorAddress, 30, 40),
    ).to.deep.equal([30n, 40n]);
    await harness.claim(operatorAddress, 30, 40);
    expect(await harness.queueLength(operatorAddress)).to.equal(0);
    expect(await harness.liveTrancheCount(operatorAddress)).to.equal(0);
  });

  it("revives an asset head when a same-timestamp merge adds that asset", async function () {
    const [operator] = await ethers.getSigners();
    const harness = await ethers.deployContract("ExitQueueHarness");
    const operatorAddress = await operator.getAddress();

    await harness.queueTicketThenLicense(operatorAddress, 0, 10, 20);
    expect(await harness.queueLength(operatorAddress)).to.equal(1);
    expect(
      await harness.claim.staticCall(operatorAddress, 10, 20),
    ).to.deep.equal([10n, 20n]);
  });

  it("caps the physical scan span when fully drained holes remain between live tranches", async function () {
    const [operator] = await ethers.getSigners();
    const harness = await ethers.deployContract("ExitQueueHarness");
    const operatorAddress = await operator.getAddress();

    // Keep live licence-only tranches between ticket-only tranches so slashing
    // the tickets creates interior holes that tail pruning cannot remove.
    let ticketTotal = 0n;
    for (let i = 0; i < 64; i++) {
      const isTicket = i % 2 === 1;
      await harness.queue(
        operatorAddress,
        i + 1,
        isTicket ? 1 : 0,
        isTicket ? 0 : 1,
      );
      if (isTicket) ticketTotal++;
    }

    await harness.slash(operatorAddress, ticketTotal, 0);
    expect(await harness.queueLength(operatorAddress)).to.equal(63);
    expect(await harness.liveTrancheCount(operatorAddress)).to.equal(32);

    // One new tranche fills the remaining span slot; another would make a
    // claim/slash scan exceed MAX_ACTIVE_TRANCHES despite only 34 live entries.
    await harness.queue(operatorAddress, 100, 1, 0);
    await expect(
      harness.queue(operatorAddress, 101, 1, 0),
    ).to.be.revertedWithCustomError(harness, "TooManyTranches");
  });
});
