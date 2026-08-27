// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";

import { deployInterfoldSystem, ethers, networkHelpers } from "../fixtures";

const { loadFixture } = networkHelpers;

describe("NodeReleaseRegistry", function () {
  async function setup() {
    return deployInterfoldSystem({ setupOperators: 1 });
  }

  it("admits every operator that acknowledges the required release", async function () {
    const { bondingRegistry } = await deployInterfoldSystem({
      setupOperators: 3,
    });
    expect(await bondingRegistry.numActiveOperators()).to.equal(3);
  });

  it("excludes a stale node after a mandatory release", async function () {
    const { interfold, bondingRegistry, nodeReleaseRegistry, operator1 } =
      await loadFixture(setup);
    const operator = await operator1!.getAddress();
    const nextRelease = ethers.id("interfold.node.release:v1:next");

    expect(await bondingRegistry.isActive(operator)).to.equal(true);
    await nodeReleaseRegistry.approveNodeRelease(nextRelease, 1, 2);
    await interfold.setRequestsPaused(true);
    await nodeReleaseRegistry.setRequiredNodeRelease(nextRelease);

    expect(await bondingRegistry.isActive(operator)).to.equal(false);
    expect(await bondingRegistry.numActiveOperators()).to.equal(0);
    expect(await nodeReleaseRegistry.isNodeReleaseReady(operator)).to.equal(
      false,
    );

    await nodeReleaseRegistry
      .connect(operator1!)
      .acknowledgeNodeRelease(nextRelease);
    expect(await bondingRegistry.isActive(operator)).to.equal(true);
    expect(await bondingRegistry.numActiveOperators()).to.equal(1);
  });

  it("keeps compatible nodes active during a recommended rollout", async function () {
    const { bondingRegistry, nodeReleaseRegistry, operator1 } =
      await loadFixture(setup);
    const operator = await operator1!.getAddress();
    const recommended = ethers.id("interfold.node.release:v1:recommended");

    await nodeReleaseRegistry.approveNodeRelease(recommended, 1, 1);
    await nodeReleaseRegistry.setRecommendedNodeRelease(recommended);

    expect(await bondingRegistry.isActive(operator)).to.equal(true);
    expect(await nodeReleaseRegistry.isNodeReleaseReady(operator)).to.equal(
      true,
    );
  });

  it("does not admit a future protocol release before its cutover", async function () {
    const { interfold, bondingRegistry, nodeReleaseRegistry, operator1 } =
      await loadFixture(setup);
    const operator = await operator1!.getAddress();
    const futureProtocol = ethers.id("interfold.node.release:v1:future");

    await nodeReleaseRegistry.approveNodeRelease(futureProtocol, 2, 1);
    await nodeReleaseRegistry
      .connect(operator1!)
      .acknowledgeNodeRelease(futureProtocol);

    expect(await nodeReleaseRegistry.isNodeReleaseReady(operator)).to.equal(
      false,
    );
    expect(await bondingRegistry.isActive(operator)).to.equal(false);
    await expect(nodeReleaseRegistry.setRecommendedNodeRelease(futureProtocol))
      .to.be.revertedWithCustomError(
        nodeReleaseRegistry,
        "NodeReleaseNotCompatible",
      )
      .withArgs(futureProtocol);

    await interfold.setRequestsPaused(true);
    await nodeReleaseRegistry.setRequiredNodeRelease(futureProtocol);
    await nodeReleaseRegistry
      .connect(operator1!)
      .acknowledgeNodeRelease(futureProtocol);
    expect(await bondingRegistry.isActive(operator)).to.equal(true);
  });

  it("requires a paused and drained cutover", async function () {
    const { nodeReleaseRegistry } = await loadFixture(setup);
    const nextRelease = ethers.id("interfold.node.release:v1:required");
    await nodeReleaseRegistry.approveNodeRelease(nextRelease, 2, 2);

    await expect(
      nodeReleaseRegistry.setRequiredNodeRelease(nextRelease),
    ).to.be.revertedWithCustomError(
      nodeReleaseRegistry,
      "NodeReleasePolicyRequiresPause",
    );
  });

  it("reserves global eligibility invalidation for the release controller", async function () {
    const { bondingRegistry, nodeReleaseRegistry } = await loadFixture(setup);

    await expect(
      bondingRegistry.refreshOperatorStatus(ethers.ZeroAddress),
    ).to.be.revertedWithCustomError(
      nodeReleaseRegistry,
      "OnlyNodeReleaseRegistry",
    );
  });

  it("keeps release metadata immutable", async function () {
    const { nodeReleaseRegistry } = await loadFixture(setup);
    const releaseId = ethers.id("interfold.node.release:v1:immutable");
    await nodeReleaseRegistry.approveNodeRelease(releaseId, 1, 2);

    await expect(nodeReleaseRegistry.approveNodeRelease(releaseId, 2, 2))
      .to.be.revertedWithCustomError(
        nodeReleaseRegistry,
        "NodeReleaseMetadataMismatch",
      )
      .withArgs(releaseId);
  });

  it("rejects a controller bound to another ciphernode registry", async function () {
    const { interfold, bondingRegistry, owner } = await loadFixture(setup);
    const wrongRegistry = await ethers.deployContract("MockCiphernodeRegistry");
    await wrongRegistry.setInterfold(await interfold.getAddress());
    const wrongController = await ethers.deployContract("NodeReleaseRegistry", [
      await owner.getAddress(),
      await bondingRegistry.getAddress(),
      await wrongRegistry.getAddress(),
    ]);

    await interfold.setRequestsPaused(true);
    await expect(
      interfold.setNodeReleaseRegistry(await wrongController.getAddress()),
    ).to.be.revertedWithCustomError(
      wrongController,
      "NodeReleaseBindingMismatch",
    );
  });

  it("cannot renounce release administration", async function () {
    const { nodeReleaseRegistry } = await loadFixture(setup);

    await expect(
      nodeReleaseRegistry.renounceOwnership(),
    ).to.be.revertedWithCustomError(
      nodeReleaseRegistry,
      "RenounceOwnershipDisabled",
    );
  });
});
