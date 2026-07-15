// SPDX-License-Identifier: LGPL-3.0-only
import { expect } from "chai";

import { ethers } from "../fixtures";

describe("InterfoldPricing storage anchors", function () {
  it("AUD-H04: writes every PricingConfig field without touching adjacent slots", async function () {
    const pricing = await ethers.deployContract("InterfoldPricing");
    const harnessFactory = await ethers.getContractFactory(
      "InterfoldPricingStorageHarness",
      { libraries: { InterfoldPricing: await pricing.getAddress() } },
    );
    const left = ethers.keccak256(ethers.toUtf8Bytes("left-slot-23"));
    const right = ethers.keccak256(ethers.toUtf8Bytes("right-slot-33"));
    const harness = await harnessFactory.deploy(left, right);

    await harness.dirtyPricing();
    await harness.applyDefaultPricingConfig();

    const config = await harness.getPricingConfig();
    expect(config.keyGenFixedPerNode).to.equal(100000n);
    expect(config.keyGenPerEncryptionProof).to.equal(50000n);
    expect(config.coordinationPerPair).to.equal(10000n);
    expect(config.availabilityPerNodePerSec).to.equal(50n);
    expect(config.decryptionPerNode).to.equal(300000n);
    expect(config.publicationBase).to.equal(1000000n);
    expect(config.verificationPerProof).to.equal(5000n);
    expect(config.protocolTreasury).to.equal(ethers.ZeroAddress);
    expect(config.marginBps).to.equal(1000);
    expect(config.protocolShareBps).to.equal(0);
    expect(config.dkgUtilizationBps).to.equal(2500);
    expect(config.computeUtilizationBps).to.equal(5000);
    expect(config.decryptUtilizationBps).to.equal(2500);
    expect(config.minCommitteeSize).to.equal(0);
    expect(config.minThreshold).to.equal(0);
    expect(await harness.leftSentinel()).to.equal(left);
    expect(await harness.rightSentinel()).to.equal(right);
  });
});
