// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";

import {
  SEVEN_DAYS,
  deployInterfoldSystem,
  ethers,
  networkHelpers,
} from "../fixtures";

const { loadFixture, time } = networkHelpers;

/// Bonded FOLD is transferred to `BondingRegistry`, which never delegates it. Under ERC20Votes an
/// undelegated balance carries no voting power, so an operator forfeits that weight entirely —
/// while the bonded tokens still count in `getPastTotalSupply` and so raise the quorum
/// denominator they cannot help meet.
///
/// `BondedCheckpoints` records bonded totals over time and `BondedVotes` sums both sources at the
/// same timepoint. These tests cover what an operator's voting power is across the bonding
/// lifecycle: bonded, unbonded, mid-exit, slashed, transferred, and delegated.
describe("BondedVotes", function () {
  this.timeout(120000);

  const BOND = ethers.parseEther("1000");
  const MINTED = ethers.parseEther("100000");

  async function setup() {
    const [, operatorKey, bondOwner, otherHolder, newOwner] =
      await ethers.getSigners();

    const sys = await deployInterfoldSystem({
      useMockCiphernodeRegistry: true,
      setupOperators: 0,
      wireSlashingManager: false,
      mintUsdcTo: [],
    });
    const { bondingRegistry, licenseToken, slashingManager } = sys;

    const bondOwnerAddress = await bondOwner.getAddress();
    const operatorAddress = await operatorKey.getAddress();
    const otherHolderAddress = await otherHolder.getAddress();
    const newOwnerAddress = await newOwner.getAddress();
    const registryAddress = await bondingRegistry.getAddress();

    for (const who of [bondOwnerAddress, otherHolderAddress, newOwnerAddress]) {
      await licenseToken.mint(
        who,
        MINTED,
        ethers.encodeBytes32String("Test allocation"),
      );
    }
    await bondingRegistry.connect(operatorKey).setBondOwner(bondOwnerAddress);

    // Self-delegate, or the wallet half of the sum is zero and nothing is being measured.
    await licenseToken.connect(bondOwner).delegate(bondOwnerAddress);
    await licenseToken.connect(otherHolder).delegate(otherHolderAddress);
    await licenseToken.connect(newOwner).delegate(newOwnerAddress);

    const checkpoints = await ethers.deployContract("BondedCheckpoints", [
      registryAddress,
    ]);
    await bondingRegistry.setBondedCheckpoints(await checkpoints.getAddress());

    const bondedVotes = await ethers.deployContract("BondedVotes", [
      await licenseToken.getAddress(),
      await checkpoints.getAddress(),
    ]);

    const bond = async (amount: bigint) => {
      await licenseToken.connect(bondOwner).approve(registryAddress, amount);
      await bondingRegistry
        .connect(bondOwner)
        .bondLicenseFor(operatorAddress, amount);
    };

    const unbond = (amount: bigint) =>
      bondingRegistry
        .connect(bondOwner)
        .unbondLicenseFor(operatorAddress, amount);

    const claim = (amount: bigint) =>
      bondingRegistry
        .connect(bondOwner)
        .claimExitsFor(operatorAddress, 0, amount);

    const slash = async (amount: bigint) => {
      const managerAddress = await slashingManager.getAddress();
      await networkHelpers.setBalance(managerAddress, ethers.parseEther("1"));
      await networkHelpers.impersonateAccount(managerAddress);
      const signer = await ethers.getSigner(managerAddress);
      await bondingRegistry
        .connect(signer)
        .slashLicenseBond(
          operatorAddress,
          amount,
          ethers.encodeBytes32String("TEST_SLASH"),
        );
      await networkHelpers.stopImpersonatingAccount(managerAddress);
    };

    /// Advance one second and read at the last settled timepoint. Every read goes through here so
    /// no test accidentally asks about a timepoint that has not settled.
    const settledVotes = async (who: string) => {
      await time.increase(1);
      return bondedVotes.getPastVotes(who, (await time.latest()) - 1);
    };

    return {
      bondingRegistry,
      licenseToken,
      checkpoints,
      bondedVotes,
      bondOwner,
      bondOwnerAddress,
      operatorKey,
      operatorAddress,
      otherHolder,
      otherHolderAddress,
      newOwner,
      newOwnerAddress,
      registryAddress,
      bond,
      unbond,
      claim,
      slash,
      settledVotes,
    };
  }

  describe("holders who never bond", function () {
    it("counts wallet FOLD exactly as the token does", async function () {
      const { licenseToken, otherHolderAddress, settledVotes } =
        await loadFixture(setup);

      expect(await settledVotes(otherHolderAddress)).to.equal(MINTED);
      expect(
        await licenseToken.getPastVotes(
          otherHolderAddress,
          (await time.latest()) - 1,
        ),
      ).to.equal(MINTED);
    });

    it("gives an address that holds nothing no power", async function () {
      const { settledVotes } = await loadFixture(setup);

      expect(await settledVotes(ethers.ZeroAddress)).to.equal(0n);
    });
  });

  describe("bonding", function () {
    it("keeps an operator's total power unchanged when it bonds", async function () {
      const {
        bondedVotes,
        licenseToken,
        bondOwnerAddress,
        bond,
        settledVotes,
      } = await loadFixture(setup);

      expect(await settledVotes(bondOwnerAddress)).to.equal(MINTED);

      await bond(BOND);

      // The whole point: bonding moves weight from wallet to bond, it does not destroy it.
      expect(await settledVotes(bondOwnerAddress)).to.equal(MINTED);

      const at = (await time.latest()) - 1;
      expect(await licenseToken.getPastVotes(bondOwnerAddress, at)).to.equal(
        MINTED - BOND,
      );
      expect(await bondedVotes.getPastVotes(bondOwnerAddress, at)).to.equal(
        MINTED,
      );
    });

    it("counts an owner whose FOLD is entirely bonded", async function () {
      const { licenseToken, bondOwnerAddress, bond, settledVotes } =
        await loadFixture(setup);

      await bond(MINTED);

      expect(await settledVotes(bondOwnerAddress)).to.equal(MINTED);
      expect(
        await licenseToken.getPastVotes(
          bondOwnerAddress,
          (await time.latest()) - 1,
        ),
      ).to.equal(0n);
    });

    it("adds up across several bonds", async function () {
      const { bondOwnerAddress, bond, settledVotes } = await loadFixture(setup);

      await bond(BOND);
      await bond(BOND);
      await bond(BOND);

      expect(await settledVotes(bondOwnerAddress)).to.equal(MINTED);
    });
  });

  describe("unbonding and exit", function () {
    /// Unbonding queues the bond for exit. The FOLD is still held by the registry, so the weight
    /// stays with the owner — counting it as wallet FOLD as well would double it.
    it("keeps power while the bond sits in the exit queue", async function () {
      const { checkpoints, bondOwnerAddress, bond, unbond, settledVotes } =
        await loadFixture(setup);

      await bond(BOND);
      await unbond(BOND);

      expect(await settledVotes(bondOwnerAddress)).to.equal(MINTED);
      expect(
        await checkpoints.getPastBonded(
          bondOwnerAddress,
          (await time.latest()) - 1,
        ),
      ).to.equal(BOND);
    });

    it("moves power back to the wallet once the exit is claimed", async function () {
      const {
        checkpoints,
        licenseToken,
        bondOwnerAddress,
        bond,
        unbond,
        claim,
        settledVotes,
      } = await loadFixture(setup);

      await bond(BOND);
      await unbond(BOND);
      await time.increase(SEVEN_DAYS + 1);
      await claim(BOND);

      // Unchanged throughout: the FOLD only ever moved between wallet and registry.
      expect(await settledVotes(bondOwnerAddress)).to.equal(MINTED);

      const at = (await time.latest()) - 1;
      expect(await checkpoints.getPastBonded(bondOwnerAddress, at)).to.equal(
        0n,
      );
      expect(await licenseToken.getPastVotes(bondOwnerAddress, at)).to.equal(
        MINTED,
      );
    });
  });

  describe("slashing", function () {
    it("reduces power by the slashed amount", async function () {
      const { checkpoints, bondOwnerAddress, bond, slash, settledVotes } =
        await loadFixture(setup);

      await bond(BOND);
      const slashAmount = BOND / 4n;
      await slash(slashAmount);

      // Slashed FOLD is gone from the owner's control, so its weight goes with it.
      expect(await settledVotes(bondOwnerAddress)).to.equal(
        MINTED - slashAmount,
      );
      expect(
        await checkpoints.getPastBonded(
          bondOwnerAddress,
          (await time.latest()) - 1,
        ),
      ).to.equal(BOND - slashAmount);
    });
  });

  describe("history", function () {
    /// A vote reads power at a snapshot. Bonding after that snapshot must not change the answer,
    /// or an owner could hold FOLD at the snapshot, bond it afterwards, and be counted twice.
    it("does not let a later bond change an earlier answer", async function () {
      const { bondedVotes, bondOwnerAddress, bond } = await loadFixture(setup);

      await time.increase(1);
      const snapshot = (await time.latest()) - 1;
      const before = await bondedVotes.getPastVotes(bondOwnerAddress, snapshot);

      await bond(BOND);
      await time.increase(1);

      expect(
        await bondedVotes.getPastVotes(bondOwnerAddress, snapshot),
      ).to.equal(before);
    });

    it("answers each timepoint with the bond held at the time", async function () {
      const { checkpoints, bondOwnerAddress, bond } = await loadFixture(setup);

      await bond(BOND);
      await time.increase(1);
      const afterFirst = (await time.latest()) - 1;

      await bond(BOND);
      await time.increase(1);
      const afterSecond = (await time.latest()) - 1;

      expect(
        await checkpoints.getPastBonded(bondOwnerAddress, afterFirst),
      ).to.equal(BOND);
      expect(
        await checkpoints.getPastBonded(bondOwnerAddress, afterSecond),
      ).to.equal(BOND * 2n);
    });

    it("reports nothing bonded before the first bond", async function () {
      const { checkpoints, bondOwnerAddress } = await loadFixture(setup);

      await time.increase(1);
      expect(
        await checkpoints.getPastBonded(
          bondOwnerAddress,
          (await time.latest()) - 1,
        ),
      ).to.equal(0n);
    });

    it("rejects a timepoint that has not settled", async function () {
      const { checkpoints, bondOwnerAddress } = await loadFixture(setup);

      await expect(
        checkpoints.getPastBonded(bondOwnerAddress, await checkpoints.clock()),
      ).to.be.revertedWithCustomError(checkpoints, "FutureLookup");
    });

    it("reports timestamps as its clock", async function () {
      const { checkpoints } = await loadFixture(setup);

      // Must match InterfoldToken, or the adapter sums two unrelated points in time.
      expect(await checkpoints.CLOCK_MODE()).to.equal("mode=timestamp");
      expect(await checkpoints.clock()).to.equal(await time.latest());
    });

    /// The invariant that catches a mutation site nobody remembered to checkpoint. The history
    /// mirrors `totalBonded` rather than replaying deltas, so a missed site shows up here rather
    /// than as silently wrong voting weight later.
    it("keeps the history equal to the live total across the whole lifecycle", async function () {
      const {
        bondingRegistry,
        checkpoints,
        bondOwnerAddress,
        bond,
        unbond,
        claim,
        slash,
      } = await loadFixture(setup);

      const assertInSync = async (label: string) => {
        await time.increase(1);
        expect(
          await checkpoints.getPastBonded(
            bondOwnerAddress,
            (await time.latest()) - 1,
          ),
          `checkpoint drifted from totalBonded after ${label}`,
        ).to.equal(await bondingRegistry.totalBonded(bondOwnerAddress));
      };

      await bond(BOND);
      await assertInSync("bond");
      await bond(BOND);
      await assertInSync("second bond");
      await slash(BOND / 2n);
      await assertInSync("slash");
      await unbond(BOND);
      await assertInSync("unbond");
      await time.increase(SEVEN_DAYS + 1);
      await claim(BOND);
      await assertInSync("claim");
    });
  });

  describe("bond-owner transfer", function () {
    it("moves bonded power to the new owner", async function () {
      const {
        bondingRegistry,
        checkpoints,
        bondOwner,
        bondOwnerAddress,
        operatorAddress,
        newOwner,
        newOwnerAddress,
        bond,
        settledVotes,
      } = await loadFixture(setup);

      await bond(BOND);
      await bondingRegistry
        .connect(bondOwner)
        .proposeBondOwner(operatorAddress, newOwnerAddress);
      await bondingRegistry.connect(newOwner).acceptBondOwner(operatorAddress);

      await time.increase(1);
      const at = (await time.latest()) - 1;

      // Both histories move together. Checkpointing only the receiver would leave the previous
      // owner voting with weight it no longer holds.
      expect(await checkpoints.getPastBonded(bondOwnerAddress, at)).to.equal(
        0n,
      );
      expect(await checkpoints.getPastBonded(newOwnerAddress, at)).to.equal(
        BOND,
      );

      expect(await settledVotes(bondOwnerAddress)).to.equal(MINTED - BOND);
      expect(await settledVotes(newOwnerAddress)).to.equal(MINTED + BOND);
    });
  });

  describe("delegation", function () {
    /// Wallet FOLD follows the token's delegation. Bonded FOLD cannot be delegated — the registry
    /// holds it — so it stays with the bond owner regardless.
    it("sends wallet weight to the delegate but keeps bonded weight", async function () {
      const {
        licenseToken,
        bondOwner,
        bondOwnerAddress,
        otherHolderAddress,
        bond,
        settledVotes,
      } = await loadFixture(setup);

      await bond(BOND);
      await licenseToken.connect(bondOwner).delegate(otherHolderAddress);

      expect(await settledVotes(bondOwnerAddress)).to.equal(BOND);
      expect(await settledVotes(otherHolderAddress)).to.equal(
        MINTED + (MINTED - BOND),
      );
    });

    /// Worth pinning: an owner who never self-delegated still gets its bonded weight, because
    /// that weight never passes through the token's delegation at all.
    it("counts bonded weight for an owner that never self-delegated", async function () {
      const { licenseToken, bondOwner, bondOwnerAddress, bond, settledVotes } =
        await loadFixture(setup);

      await bond(BOND);
      await licenseToken.connect(bondOwner).delegate(ethers.ZeroAddress);

      expect(await settledVotes(bondOwnerAddress)).to.equal(BOND);
    });

    it("refuses to delegate through the adapter", async function () {
      const { bondedVotes, bondOwnerAddress } = await loadFixture(setup);

      // Reverting stops a caller believing it moved weight that never moved.
      await expect(
        bondedVotes.delegate(bondOwnerAddress),
      ).to.be.revertedWithCustomError(bondedVotes, "DelegationNotSupported");
    });
  });

  describe("total supply", function () {
    /// Bonded FOLD was transferred, not burned, so it is already in the supply. Adding it again
    /// would inflate every quorum denominator — the opposite of the problem being fixed.
    it("passes total supply through unchanged while bonded", async function () {
      const { bondedVotes, licenseToken, bond } = await loadFixture(setup);

      const before = await licenseToken.totalSupply();
      await bond(BOND);
      await time.increase(1);

      const at = (await time.latest()) - 1;
      expect(await bondedVotes.getPastTotalSupply(at)).to.equal(before);
      expect(await bondedVotes.getPastTotalSupply(at)).to.equal(
        await licenseToken.getPastTotalSupply(at),
      );
    });

    /// Quorum is a fraction of supply, so summed power must never exceed it — counting the same
    /// FOLD twice is the failure this whole design exists to avoid.
    it("never lets summed power exceed total supply", async function () {
      const {
        bondedVotes,
        licenseToken,
        bondOwnerAddress,
        otherHolderAddress,
        newOwnerAddress,
        bond,
      } = await loadFixture(setup);

      await bond(BOND);
      await time.increase(1);
      const at = (await time.latest()) - 1;

      const summed =
        (await bondedVotes.getPastVotes(bondOwnerAddress, at)) +
        (await bondedVotes.getPastVotes(otherHolderAddress, at)) +
        (await bondedVotes.getPastVotes(newOwnerAddress, at));

      expect(summed).to.be.lessThanOrEqual(
        await licenseToken.getPastTotalSupply(at),
      );
    });
  });

  describe("current voting power", function () {
    /// `getVotes` must read both halves at the same instant. Pairing a current wallet balance
    /// with a timepoint-behind bonded read would leave the bonded half stale after a claim in the
    /// same block, and the sum could exceed what the owner actually holds.
    it("reflects a claim in the same block", async function () {
      const { bondedVotes, bondOwnerAddress, bond, unbond, claim } =
        await loadFixture(setup);

      await bond(BOND);
      await unbond(BOND);
      await time.increase(SEVEN_DAYS + 1);
      await claim(BOND);

      // Read immediately, without advancing time.
      expect(await bondedVotes.getVotes(bondOwnerAddress)).to.equal(MINTED);
    });

    it("reflects a slash in the same block", async function () {
      const { bondedVotes, bondOwnerAddress, bond, slash } =
        await loadFixture(setup);

      await bond(BOND);
      const slashAmount = BOND / 4n;
      await slash(slashAmount);

      expect(await bondedVotes.getVotes(bondOwnerAddress)).to.equal(
        MINTED - slashAmount,
      );
    });

    it("agrees with the historical read once the timepoint settles", async function () {
      const { bondedVotes, bondOwnerAddress, bond } = await loadFixture(setup);

      await bond(BOND);
      const now = await bondedVotes.getVotes(bondOwnerAddress);

      await time.increase(1);
      expect(
        await bondedVotes.getPastVotes(
          bondOwnerAddress,
          (await time.latest()) - 1,
        ),
      ).to.equal(now);
    });
  });

  describe("history predating configuration", function () {
    /// The sync is a no-op while `bondedCheckpoints` is unset, so an owner that bonded before
    /// configuration has no history until its next mutation. `resyncBondedCheckpoint` repairs
    /// that without waiting for one.
    it("records an owner that bonded before the checkpoint contract existed", async function () {
      const [, operatorKey, bondOwner] = await ethers.getSigners();

      const sys = await deployInterfoldSystem({
        useMockCiphernodeRegistry: true,
        setupOperators: 0,
        wireSlashingManager: false,
        mintUsdcTo: [],
      });
      const { bondingRegistry, licenseToken } = sys;

      const bondOwnerAddress = await bondOwner.getAddress();
      const operatorAddress = await operatorKey.getAddress();
      const registryAddress = await bondingRegistry.getAddress();

      await licenseToken.mint(
        bondOwnerAddress,
        MINTED,
        ethers.encodeBytes32String("Test allocation"),
      );
      await bondingRegistry.connect(operatorKey).setBondOwner(bondOwnerAddress);

      // Bond first, with no checkpoint contract configured.
      await licenseToken.connect(bondOwner).approve(registryAddress, BOND);
      await bondingRegistry
        .connect(bondOwner)
        .bondLicenseFor(operatorAddress, BOND);

      const checkpoints = await ethers.deployContract("BondedCheckpoints", [
        registryAddress,
      ]);
      await bondingRegistry.setBondedCheckpoints(
        await checkpoints.getAddress(),
      );

      // Configuration alone backfills nothing.
      expect(await checkpoints.bonded(bondOwnerAddress)).to.equal(0n);

      // Permissionless: a third party can repair an owner's history.
      await bondingRegistry
        .connect(operatorKey)
        .resyncBondedCheckpoint(bondOwnerAddress);

      expect(await checkpoints.bonded(bondOwnerAddress)).to.equal(BOND);
      expect(await bondingRegistry.totalBonded(bondOwnerAddress)).to.equal(BOND);
    });

    it("is idempotent", async function () {
      const { bondingRegistry, checkpoints, bondOwnerAddress, bond } =
        await loadFixture(setup);

      await bond(BOND);
      await bondingRegistry.resyncBondedCheckpoint(bondOwnerAddress);
      await bondingRegistry.resyncBondedCheckpoint(bondOwnerAddress);

      // It can only ever write the true current total, so repeating it changes nothing.
      expect(await checkpoints.bonded(bondOwnerAddress)).to.equal(BOND);
    });
  });

  describe("wiring", function () {
    it("rejects a checkpoint contract bound to another registry", async function () {
      // Deliberately NOT the shared fixture: that one already configures a checkpoint contract,
      // so the one-shot guard would fire first and this would assert the repoint path instead.
      // Both revert with InvalidConfiguration, so the mismatch branch would go uncovered.
      const { bondingRegistry } = await deployInterfoldSystem({
        useMockCiphernodeRegistry: true,
        setupOperators: 0,
        wireSlashingManager: false,
        mintUsdcTo: [],
      });

      const foreign = await ethers.deployContract("BondedCheckpoints", [
        ethers.Wallet.createRandom().address,
      ]);

      // A mismatch would make every sync revert and brick bonding for good.
      await expect(
        bondingRegistry.setBondedCheckpoints(await foreign.getAddress()),
      ).to.be.revertedWithCustomError(bondingRegistry, "InvalidConfiguration");

      // Proves the revert above came from the registry cross-check and not the one-shot guard:
      // the slot is still unset, so a correctly bound contract is still accepted.
      const owned = await ethers.deployContract("BondedCheckpoints", [
        await bondingRegistry.getAddress(),
      ]);
      await expect(bondingRegistry.setBondedCheckpoints(await owned.getAddress()))
        .to.emit(bondingRegistry, "BondedCheckpointsSet")
        .withArgs(await owned.getAddress());
    });

    it("refuses to be repointed once set", async function () {
      const { bondingRegistry, registryAddress } = await loadFixture(setup);

      const replacement = await ethers.deployContract("BondedCheckpoints", [
        registryAddress,
      ]);

      // Repointing would abandon the recorded history, silently changing every past answer.
      await expect(
        bondingRegistry.setBondedCheckpoints(await replacement.getAddress()),
      ).to.be.revertedWithCustomError(bondingRegistry, "InvalidConfiguration");
    });

    it("only lets the registry write history", async function () {
      const { checkpoints, bondOwnerAddress, otherHolder } =
        await loadFixture(setup);

      await expect(
        (checkpoints.connect(otherHolder) as typeof checkpoints).sync(
          bondOwnerAddress,
          BOND,
        ),
      ).to.be.revertedWithCustomError(checkpoints, "OnlyRegistry");
    });
  });
});
