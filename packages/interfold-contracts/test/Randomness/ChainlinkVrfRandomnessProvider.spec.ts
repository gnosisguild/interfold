// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";

import { ethers, networkHelpers } from "../fixtures";

describe("ChainlinkVrfRandomnessProvider", function () {
  async function setup() {
    const [owner, protocolOwner, other] = await ethers.getSigners();
    const requester = await ethers.deployContract("MockCiphernodeRegistry");
    await requester.waitForDeployment();

    const coordinator = await ethers.deployContract(
      "ChainlinkVrfCoordinatorV2_5Mock",
      [0, 0, ethers.parseEther("1")],
    );
    await coordinator.waitForDeployment();
    await coordinator.createSubscription();
    const [subscriptionId] = await coordinator.getActiveSubscriptionIds(0, 1);
    if (!subscriptionId) throw new Error("subscription missing");
    await coordinator.fundSubscription(
      subscriptionId,
      ethers.parseEther("100"),
    );

    const provider: any = await ethers.deployContract(
      "ChainlinkVrfRandomnessProvider",
      [
        await requester.getAddress(),
        await coordinator.getAddress(),
        subscriptionId,
        `0x${"11".repeat(32)}`,
        3,
        500_000,
        false,
        await protocolOwner.getAddress(),
      ],
    );
    await provider.waitForDeployment();
    await coordinator.addConsumer(subscriptionId, await provider.getAddress());

    const requesterAddress = await requester.getAddress();
    await networkHelpers.impersonateAccount(requesterAddress);
    await networkHelpers.setBalance(requesterAddress, ethers.parseEther("1"));
    const requesterSigner = await ethers.getSigner(requesterAddress);

    return {
      coordinator,
      other,
      owner,
      protocolOwner,
      provider,
      requesterAddress,
      requesterSigner,
      subscriptionId,
    };
  }

  it("records the response for the matching subscription request", async function () {
    const { coordinator, provider, requesterSigner } = await setup();
    const e3Id = 42n;
    const randomWord = 123456789n;

    await expect(provider.connect(requesterSigner).requestRandomness(e3Id))
      .to.emit(provider, "RandomnessRequested")
      .withArgs(1, e3Id);
    const fulfillment = await coordinator.fulfillRandomWordsWithOverride(
      1,
      await provider.getAddress(),
      [randomWord],
    );
    const fulfillmentReceipt = await fulfillment.wait();
    if (!fulfillmentReceipt) throw new Error("fulfillment receipt missing");

    const [fulfilled, storedWord, fulfilledAt, fulfilledBlock] =
      await provider.getRandomness(1);
    expect(fulfilled).to.equal(true);
    expect(storedWord).to.equal(randomWord);
    expect(fulfilledAt).to.be.greaterThan(0);
    expect(fulfilledBlock).to.equal(BigInt(fulfillmentReceipt.blockNumber));
  });

  it("allows only the bound registry to request and never re-requests an E3", async function () {
    const { other, provider, requesterSigner } = await setup();

    await expect(provider.connect(other).requestRandomness(7))
      .to.be.revertedWithCustomError(provider, "OnlyRequester")
      .withArgs(await other.getAddress());
    await provider.connect(requesterSigner).requestRandomness(7);
    await expect(provider.connect(requesterSigner).requestRandomness(7))
      .to.be.revertedWithCustomError(provider, "RandomnessAlreadyRequested")
      .withArgs(7);
  });

  it("does not revert the coordinator callback for an unknown response", async function () {
    const { coordinator, provider } = await setup();
    const coordinatorAddress = await coordinator.getAddress();
    await networkHelpers.impersonateAccount(coordinatorAddress);
    await networkHelpers.setBalance(coordinatorAddress, ethers.parseEther("1"));
    const coordinatorSigner = await ethers.getSigner(coordinatorAddress);

    await expect(
      provider.connect(coordinatorSigner).rawFulfillRandomWords(999, [1]),
    )
      .to.emit(provider, "RandomnessResponseIgnored")
      .withArgs(999);
  });

  it("keeps the first valid response when the coordinator calls back twice", async function () {
    const { coordinator, provider, requesterSigner } = await setup();
    await provider.connect(requesterSigner).requestRandomness(7);
    await coordinator.fulfillRandomWordsWithOverride(
      1,
      await provider.getAddress(),
      [111],
    );

    const coordinatorAddress = await coordinator.getAddress();
    await networkHelpers.impersonateAccount(coordinatorAddress);
    await networkHelpers.setBalance(coordinatorAddress, ethers.parseEther("1"));
    const coordinatorSigner = await ethers.getSigner(coordinatorAddress);
    await expect(
      provider.connect(coordinatorSigner).rawFulfillRandomWords(1, [222]),
    )
      .to.emit(provider, "RandomnessResponseIgnored")
      .withArgs(1);

    const [fulfilled, randomWord] = await provider.getRandomness(1);
    expect(fulfilled).to.equal(true);
    expect(randomWord).to.equal(111);
  });

  it("hands coordinator migration control to the protocol owner", async function () {
    const { owner, protocolOwner, provider } = await setup();
    expect(await provider.owner()).to.equal(await owner.getAddress());
    await provider.connect(protocolOwner).acceptOwnership();
    expect(await provider.owner()).to.equal(await protocolOwner.getAddress());
  });
});
