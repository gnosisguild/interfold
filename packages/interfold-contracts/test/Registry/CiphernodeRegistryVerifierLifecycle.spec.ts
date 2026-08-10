// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";

import { deployInterfoldSystem, ethers, networkHelpers } from "../fixtures";

const { loadFixture, time } = networkHelpers;

describe("CiphernodeRegistryOwnable verifier lifecycle", function () {
  const setup = async () => {
    const sys = await deployInterfoldSystem({
      wireMockDkgFoldAttestationVerifier: false,
    });
    const verifier1 = await ethers.deployContract("DkgFoldAttestationVerifier");
    const verifier2 = await ethers.deployContract("DkgFoldAttestationVerifier");
    const verifier3 = await ethers.deployContract("DkgFoldAttestationVerifier");
    return {
      owner: sys.owner,
      notTheOwner: sys.notTheOwner,
      registry: sys.ciphernodeRegistry,
      verifier1,
      verifier2,
      verifier3,
    };
  };

  it("supports propose/commit with timelock enforcement", async function () {
    const { registry, verifier1, verifier2 } = await loadFixture(setup);

    await registry.setInitialDkgFoldAttestationVerifier(
      await verifier1.getAddress(),
    );

    const timelock = await registry.DKG_FOLD_VERIFIER_TIMELOCK();

    await expect(
      registry.proposeDkgFoldAttestationVerifier(await verifier2.getAddress()),
    )
      .to.emit(registry, "DkgFoldAttestationVerifierProposed")
      .withArgs(
        await verifier2.getAddress(),
        (readyAt: bigint) => readyAt > 0n,
      );

    await expect(
      registry.commitDkgFoldAttestationVerifier(await verifier2.getAddress()),
    ).to.be.revertedWithCustomError(registry, "VerifierUpdateTimelockActive");

    await time.increase(Number(timelock) + 1);

    await expect(
      registry.commitDkgFoldAttestationVerifier(await verifier2.getAddress()),
    )
      .to.emit(registry, "DkgFoldAttestationVerifierUpdated")
      .withArgs(await verifier2.getAddress());

    expect(await registry.dkgFoldAttestationVerifier()).to.equal(
      await verifier2.getAddress(),
    );
  });

  it("rejects commit with verifier mismatch", async function () {
    const { registry, verifier1, verifier2, verifier3 } =
      await loadFixture(setup);

    await registry.setInitialDkgFoldAttestationVerifier(
      await verifier1.getAddress(),
    );
    await registry.proposeDkgFoldAttestationVerifier(
      await verifier2.getAddress(),
    );
    await time.increase(
      Number(await registry.DKG_FOLD_VERIFIER_TIMELOCK()) + 1,
    );

    await expect(
      registry.commitDkgFoldAttestationVerifier(await verifier3.getAddress()),
    )
      .to.be.revertedWithCustomError(registry, "VerifierMismatch")
      .withArgs(await verifier2.getAddress(), await verifier3.getAddress());
  });

  it("cleans up stale pending proposal on setInitial", async function () {
    const { registry, verifier1, verifier2 } = await loadFixture(setup);

    await registry.proposeDkgFoldAttestationVerifier(
      await verifier2.getAddress(),
    );

    await expect(
      registry.setInitialDkgFoldAttestationVerifier(
        await verifier1.getAddress(),
      ),
    )
      .to.emit(registry, "DkgFoldAttestationVerifierProposalCancelled")
      .withArgs(await verifier2.getAddress())
      .and.to.emit(registry, "DkgFoldAttestationVerifierUpdated")
      .withArgs(await verifier1.getAddress());

    expect(await registry.pendingDkgFoldAttestationVerifier()).to.equal(
      ethers.ZeroAddress,
    );
    expect(await registry.pendingDkgFoldAttestationVerifierAt()).to.equal(0);

    await expect(
      registry.commitDkgFoldAttestationVerifier(await verifier2.getAddress()),
    ).to.be.revertedWithCustomError(registry, "NoPendingVerifierUpdate");
  });

  it("cancels a pending proposal", async function () {
    const { registry, verifier2 } = await loadFixture(setup);

    await registry.proposeDkgFoldAttestationVerifier(
      await verifier2.getAddress(),
    );

    await expect(registry.cancelDkgFoldAttestationVerifierProposal())
      .to.emit(registry, "DkgFoldAttestationVerifierProposalCancelled")
      .withArgs(await verifier2.getAddress());

    expect(await registry.pendingDkgFoldAttestationVerifier()).to.equal(
      ethers.ZeroAddress,
    );
    expect(await registry.pendingDkgFoldAttestationVerifierAt()).to.equal(0);
  });

  it("requires the timelock for any accusationVoteValidity reduction", async function () {
    const { registry } = await loadFixture(setup);

    await expect(
      registry.setAccusationVoteValidity(1),
    ).to.be.revertedWithCustomError(
      registry,
      "AccusationVoteValidityDecreaseRequiresTimelock",
    );

    await registry.proposeAccusationVoteValidity(1);
    await expect(
      registry.commitAccusationVoteValidity(1),
    ).to.be.revertedWithCustomError(
      registry,
      "AccusationVoteValidityTimelockActive",
    );

    await time.increase(
      Number(await registry.ACCUSATION_VOTE_VALIDITY_TIMELOCK()) + 1,
    );

    await expect(registry.commitAccusationVoteValidity(1))
      .to.emit(registry, "AccusationVoteValiditySet")
      .withArgs(1);
    expect(await registry.accusationVoteValidity()).to.equal(1);
  });

  it("allows accusationVoteValidity increases without the timelock", async function () {
    const { registry } = await loadFixture(setup);
    const increased = (await registry.DEFAULT_ACCUSATION_VOTE_VALIDITY()) + 1n;

    await expect(registry.setAccusationVoteValidity(increased))
      .to.emit(registry, "AccusationVoteValiditySet")
      .withArgs(increased);
    expect(await registry.accusationVoteValidity()).to.equal(increased);
  });

  it("clears a pending reduction on a direct validity update", async function () {
    const { registry } = await loadFixture(setup);
    const increased = (await registry.DEFAULT_ACCUSATION_VOTE_VALIDITY()) + 1n;

    await registry.proposeAccusationVoteValidity(0);
    await expect(registry.setAccusationVoteValidity(increased))
      .to.emit(registry, "AccusationVoteValidityProposalCancelled")
      .withArgs(0)
      .and.to.emit(registry, "AccusationVoteValiditySet")
      .withArgs(increased);

    expect(await registry.pendingAccusationVoteValidityAt()).to.equal(0);
    await expect(
      registry.commitAccusationVoteValidity(0),
    ).to.be.revertedWithCustomError(
      registry,
      "NoPendingAccusationVoteValidityUpdate",
    );
  });

  it("expires an uncommitted accusationVoteValidity proposal", async function () {
    const { registry } = await loadFixture(setup);
    const timelock = await registry.ACCUSATION_VOTE_VALIDITY_TIMELOCK();

    await registry.proposeAccusationVoteValidity(0);
    await time.increase(Number(timelock * 2n) + 1);

    await expect(
      registry.commitAccusationVoteValidity(0),
    ).to.be.revertedWithCustomError(
      registry,
      "AccusationVoteValidityProposalExpired",
    );
  });

  it("cancels pending accusationVoteValidity proposal", async function () {
    const { registry } = await loadFixture(setup);

    await registry.proposeAccusationVoteValidity(1234);
    await expect(registry.cancelAccusationVoteValidityProposal())
      .to.emit(registry, "AccusationVoteValidityProposalCancelled")
      .withArgs(1234);

    expect(await registry.pendingAccusationVoteValidity()).to.equal(0);
    expect(await registry.pendingAccusationVoteValidityAt()).to.equal(0);
  });
});
