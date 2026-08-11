// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";
import type { Signer } from "ethers";

import { CiphernodeRegistryOwnable__factory as CiphernodeRegistryFactory } from "../../types";
import {
  ADDRESS_ONE as AddressOne,
  ADDRESS_TWO as AddressTwo,
  SEVEN_DAYS,
  TICKET_PRICE,
  deployInterfoldSystem,
  encodeMockDkgProof,
  ethers,
  networkHelpers,
  setBondingAssetConfig,
  setupOperatorForSortition,
} from "../fixtures";

const { loadFixture } = networkHelpers;

const data = "0xda7a";
const dataHash = ethers.id(data);
const SORTITION_SUBMISSION_WINDOW = 10;

describe("CiphernodeRegistryOwnable", function () {
  async function finalizeCommitteeAfterWindow(
    registry: any,
    e3Id: number,
  ): Promise<void> {
    await networkHelpers.time.increase(SORTITION_SUBMISSION_WINDOW + 1);
    await registry.finalizeCommittee(e3Id);
  }

  async function setup() {
    const sys = await deployInterfoldSystem({
      submissionWindow: SORTITION_SUBMISSION_WINDOW,
      committeeThresholds: [[0, [2, 3]]],
    });
    const request = (signer?: Signer) =>
      makeRequest(
        sys.interfold,
        sys.usdcToken,
        sys.mocks.e3Program,
        sys.mocks.decryptionVerifier,
        signer,
      );
    return {
      owner: sys.owner,
      notTheOwner: sys.notTheOwner,
      operator1: sys.operator1!,
      operator2: sys.operator2!,
      operator3: sys.operator3!,
      registry: sys.ciphernodeRegistry,
      interfold: sys.interfold,
      slashingManager: sys.slashingManager,
      bondingRegistry: sys.bondingRegistry,
      licenseToken: sys.licenseToken,
      ticketToken: sys.ticketToken,
      usdcToken: sys.usdcToken,
      mockE3Program: sys.mocks.e3Program,
      mockDecryptionVerifier: sys.mocks.decryptionVerifier,
      mockPkVerifier: sys.mocks.pkVerifier,
      request,
    };
  }

  // Helper to make a request through the Interfold contract
  async function makeRequest(
    interfold: any,
    usdcToken: any,
    mockE3Program: any,
    mockDecryptionVerifier: any,
    signer?: Signer,
  ) {
    const abiCoder = ethers.AbiCoder.defaultAbiCoder();

    const currentTime = await networkHelpers.time.latest();
    const requestParams = {
      committeeSize: 0,
      inputWindow: [currentTime + 100, currentTime + 300] as [number, number],
      e3Program: await mockE3Program.getAddress(),
      paramSet: 0,
      computeProviderParams: abiCoder.encode(
        ["address"],
        [await mockDecryptionVerifier.getAddress()],
      ),
      customParams: abiCoder.encode(
        ["address"],
        ["0x1234567890123456789012345678901234567890"],
      ),
    };

    const fee = await interfold.getE3Quote(requestParams);
    const tokenContract = signer ? usdcToken.connect(signer) : usdcToken;
    const interfoldContract = signer ? interfold.connect(signer) : interfold;

    await tokenContract.approve(await interfold.getAddress(), fee);
    return interfoldContract.request(requestParams);
  }

  describe("constructor / initialize()", function () {
    it("correctly sets `_owner` and `interfold` ", async function () {
      const poseidonFactory = await ethers.getContractFactory("PoseidonT3");
      const poseidonDeployment = await poseidonFactory.deploy();
      await poseidonDeployment.waitForDeployment();
      const poseidonAddress = await poseidonDeployment.getAddress();
      const [deployer] = await ethers.getSigners();
      if (!deployer) throw new Error("Bad getSigners() output");

      const ciphernodeRegistryFactory = await ethers.getContractFactory(
        "CiphernodeRegistryOwnable",
        {
          libraries: {
            PoseidonT3: poseidonAddress,
          },
        },
      );
      const implementation = await ciphernodeRegistryFactory.deploy();
      await implementation.waitForDeployment();
      const implementationAddress = await implementation.getAddress();

      const initData = ciphernodeRegistryFactory.interface.encodeFunctionData(
        "initialize",
        [deployer.address, SORTITION_SUBMISSION_WINDOW],
      );

      const proxyFactory = await ethers.getContractFactory(
        "TransparentUpgradeableProxy",
      );
      const proxy = await proxyFactory.deploy(
        implementationAddress,
        deployer.address,
        initData,
      );
      await proxy.waitForDeployment();
      const proxyAddress = await proxy.getAddress();

      const ciphernodeRegistry = CiphernodeRegistryFactory.connect(
        proxyAddress,
        deployer,
      );

      expect(await ciphernodeRegistry.owner()).to.equal(deployer.address);
      expect(await ciphernodeRegistry.sortitionSubmissionWindow()).to.equal(
        SORTITION_SUBMISSION_WINDOW,
      );
    });
  });

  describe("requestCommittee()", function () {
    it("stores rootAt for the requested e3Id after a successful request", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      } = await loadFixture(setup);
      // Request through Interfold
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );
      expect(await registry.rootAt(0)).to.equal(await registry.root());
    });
    it("stores the root of the ciphernode registry at the time of the request", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      } = await loadFixture(setup);
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );
      expect(await registry.rootAt(0)).to.equal(await registry.root());
    });
    it("emits a CommitteeRequested event", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      } = await loadFixture(setup);

      const tx = await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );

      // Should emit CommitteeRequested from registry
      await expect(tx).to.emit(registry, "CommitteeRequested");
    });
    it("returns true if the request is successful", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      } = await loadFixture(setup);
      // We can verify by checking that root is stored after request
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );
      expect(await registry.rootAt(0)).to.not.equal(0);
    });

    it("allows one ticket ID across concurrent E3 requests", async function () {
      const { registry, operator1, request } = await loadFixture(setup);

      for (let e3Id = 0; e3Id < 2; e3Id++) {
        await request();
        await registry.connect(operator1).submitTicket(e3Id, 1);
      }
    });

    it("uses one ticket price for the full submission window", async function () {
      const {
        owner,
        registry,
        bondingRegistry,
        operator1,
        operator2,
        operator3,
        request,
      } = await loadFixture(setup);

      await registry.connect(owner).setSortitionSubmissionWindow(30);
      await request();
      expect(await registry.sortitionTicketPrices(0)).to.equal(TICKET_PRICE);

      await setBondingAssetConfig(bondingRegistry, {
        ticketPrice: TICKET_PRICE * 2n,
      });
      await bondingRegistry.refreshOperatorStatuses([
        await operator1.getAddress(),
        await operator2.getAddress(),
        await operator3.getAddress(),
      ]);
      await registry.connect(operator1).submitTicket(0, 10);

      await setBondingAssetConfig(bondingRegistry, {
        ticketPrice: TICKET_PRICE / 2n,
      });
      await bondingRegistry.refreshOperatorStatuses([
        await operator1.getAddress(),
        await operator2.getAddress(),
        await operator3.getAddress(),
      ]);

      await expect(
        registry.connect(operator2).submitTicket.staticCall(0, 11),
      ).to.be.revertedWithCustomError(registry, "InvalidTicketNumber");
      await registry.connect(operator2).submitTicket(0, 10);
    });

    it("does not admit an operator activated after the seed is known", async function () {
      const {
        owner,
        registry,
        bondingRegistry,
        licenseToken,
        ticketToken,
        usdcToken,
        request,
      } = await loadFixture(setup);
      const signers = await ethers.getSigners();
      const lateOperator = signers[5];
      const lateOperatorAddress = await lateOperator.getAddress();

      await setupOperatorForSortition(
        lateOperator,
        owner,
        bondingRegistry,
        licenseToken,
        usdcToken,
        ticketToken,
        registry,
      );
      await bondingRegistry
        .connect(owner)
        .unbondLicenseFor(lateOperatorAddress, ethers.parseEther("1000"));
      await networkHelpers.time.increase(SEVEN_DAYS + 1);

      const tx = await request();
      const receipt = await tx.wait();
      const event = receipt!.logs
        .map((log: any) => {
          try {
            return registry.interface.parseLog(log);
          } catch {
            return null;
          }
        })
        .find((log: any) => log?.name === "CommitteeRequested");
      const requestBlock = event!.args.requestBlock as bigint;

      await bondingRegistry
        .connect(owner)
        .bondLicenseFor(lateOperatorAddress, ethers.parseEther("1000"));
      expect(await bondingRegistry.isActive(lateOperatorAddress)).to.equal(
        true,
      );
      const [activeAtRequest] = await bondingRegistry.eligibilityAt(
        lateOperatorAddress,
        requestBlock - 1n,
      );
      expect(activeAtRequest).to.equal(false);

      await expect(
        registry.connect(lateOperator).submitTicket(0, 1),
      ).to.be.revertedWithCustomError(registry, "NodeNotEligible");
    });

    it("AUD-M03: fails closed after governance updates until operators refresh", async function () {
      const {
        registry,
        interfold,
        bondingRegistry,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
        operator1,
        operator2,
        operator3,
      } = await loadFixture(setup);

      await bondingRegistry.setLicenseActiveBps(9_000);
      expect(await bondingRegistry.numActiveOperators()).to.equal(0);

      await expect(
        makeRequest(
          interfold,
          usdcToken,
          mockE3Program,
          mockDecryptionVerifier,
        ),
      )
        .to.be.revertedWithCustomError(registry, "InsufficientCiphernodes")
        .withArgs(3, 0);

      await bondingRegistry.refreshOperatorStatuses([
        await operator1.getAddress(),
        await operator2.getAddress(),
        await operator3.getAddress(),
      ]);
      expect(await bondingRegistry.numActiveOperators()).to.equal(3);

      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );
      expect(await registry.rootAt(0)).to.equal(await registry.root());
    });

    it("rejects tickets from an operator banned after registration", async function () {
      const {
        owner,
        notTheOwner,
        operator1,
        registry,
        interfold,
        slashingManager,
        bondingRegistry,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      } = await loadFixture(setup);
      const operator = await operator1.getAddress();
      const reason = ethers.encodeBytes32String("manual_ban");
      const governanceRole = await slashingManager.GOVERNANCE_ROLE();

      await slashingManager
        .connect(owner)
        .grantRole(governanceRole, await notTheOwner.getAddress());
      await slashingManager.connect(owner).proposeBan(operator, reason);
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );
      expect(await bondingRegistry.isActive(operator)).to.equal(true);
      expect(await bondingRegistry.numActiveOperators()).to.equal(3);

      await expect(
        slashingManager.connect(notTheOwner).confirmBan(operator, reason),
      )
        .to.emit(bondingRegistry, "OperatorActivationChanged")
        .withArgs(operator, false);

      expect(await bondingRegistry.isActive(operator)).to.equal(false);
      expect(await bondingRegistry.numActiveOperators()).to.equal(2);
      expect(await registry.isCiphernodeEligible(operator)).to.equal(false);
      await expect(
        registry.connect(operator1).submitTicket(0, 1),
      ).to.be.revertedWithCustomError(registry, "NodeNotEligible");

      await expect(slashingManager.connect(owner).unbanNode(operator, reason))
        .to.emit(bondingRegistry, "OperatorActivationChanged")
        .withArgs(operator, true);
      expect(await bondingRegistry.isActive(operator)).to.equal(true);
      expect(await bondingRegistry.numActiveOperators()).to.equal(3);
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );
      await registry.connect(operator1).submitTicket(1, 1);
    });
  });

  describe("publishCommittee()", function () {
    it("keeps each E3 on its request-time fold verifier after rotation", async function () {
      const {
        owner,
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      } = await loadFixture(setup);
      const oldVerifier = await registry.dkgFoldAttestationVerifier();

      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );

      const newVerifier = await ethers.deployContract(
        "MockDkgFoldAttestationVerifier",
      );
      await newVerifier.waitForDeployment();
      await registry
        .connect(owner)
        .proposeDkgFoldAttestationVerifier(await newVerifier.getAddress());
      await networkHelpers.time.increase(
        Number(await registry.DKG_FOLD_VERIFIER_TIMELOCK()) + 1,
      );
      await registry
        .connect(owner)
        .commitDkgFoldAttestationVerifier(await newVerifier.getAddress());

      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );

      expect(await registry.dkgFoldAttestationVerifierFor(0)).to.equal(
        oldVerifier,
      );
      expect(await registry.dkgFoldAttestationVerifierFor(1)).to.equal(
        await newVerifier.getAddress(),
      );
      const contextEvents = await registry.queryFilter(
        registry.filters.DkgFoldAttestationContextEstablished(),
      );
      expect(contextEvents.map((event) => event.args.e3Id)).to.deep.equal([
        0n,
        1n,
      ]);
      expect(contextEvents.map((event) => event.args.registry)).to.deep.equal([
        await registry.getAddress(),
        await registry.getAddress(),
      ]);
      expect(
        contextEvents.map((event) => event.args.dkgFoldAttestationVerifier),
      ).to.deep.equal([oldVerifier, await newVerifier.getAddress()]);
    });

    it("AUD-C02: requires a final DKG proof and attestation bundle", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
        operator1,
        operator2,
        operator3,
      } = await loadFixture(setup);
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );
      await registry.connect(operator1).submitTicket(0, 1);
      await registry.connect(operator2).submitTicket(0, 1);
      await registry.connect(operator3).submitTicket(0, 1);
      await finalizeCommitteeAfterWindow(registry, 0);

      await expect(
        registry.publishCommittee(0, dataHash, "0x", "0x"),
      ).to.be.revertedWithCustomError(registry, "DkgProofRequired");
      await expect(
        registry.publishCommittee(
          0,
          dataHash,
          encodeMockDkgProof(dataHash),
          "0x",
        ),
      ).to.be.revertedWithCustomError(registry, "FoldAttestationsRequired");
    });
    it("rejects a false public-key verifier result", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
        mockPkVerifier,
        operator1,
        operator2,
        operator3,
      } = await loadFixture(setup);
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );
      await registry.connect(operator1).submitTicket(0, 1);
      await registry.connect(operator2).submitTicket(0, 1);
      await registry.connect(operator3).submitTicket(0, 1);
      await finalizeCommitteeAfterWindow(registry, 0);

      const falseProof = ethers.AbiCoder.defaultAbiCoder().encode(
        ["bytes", "bytes32[]"],
        ["0xfafafafa", [dataHash]],
      );
      await expect(
        registry.publishCommittee(0, dataHash, falseProof, "0x01"),
      ).to.be.revertedWithCustomError(mockPkVerifier, "InvalidProof");
    });
    it("allows any caller to publish a finalized committee proof", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
        notTheOwner,
        operator1,
        operator2,
        operator3,
      } = await loadFixture(setup);
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );

      await registry.connect(operator1).submitTicket(0, 1);
      await registry.connect(operator2).submitTicket(0, 1);
      await registry.connect(operator3).submitTicket(0, 1);
      await finalizeCommitteeAfterWindow(registry, 0);

      await expect(
        registry
          .connect(notTheOwner)
          .publishCommittee(0, dataHash, encodeMockDkgProof(dataHash), "0x01"),
      )
        .to.emit(registry, "CommitteeProofPublished")
        .withArgs(
          0,
          [
            await operator3.getAddress(),
            await operator1.getAddress(),
            await operator2.getAddress(),
          ],
          dataHash,
          encodeMockDkgProof(dataHash),
        );
    });
    it("stores the public key of the committee", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
        operator1,
        operator2,
        operator3,
      } = await loadFixture(setup);
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );

      await registry.connect(operator1).submitTicket(0, 1);
      await registry.connect(operator2).submitTicket(0, 1);
      await registry.connect(operator3).submitTicket(0, 1);
      await finalizeCommitteeAfterWindow(registry, 0);

      await registry.publishCommittee(
        0,
        dataHash,
        encodeMockDkgProof(dataHash),
        "0x01",
      );
      expect(await registry.committeePublicKey(0)).to.equal(dataHash);
    });
    it("lets a valid public-key candidate follow an invalid one", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
        operator1,
        operator2,
        operator3,
        notTheOwner,
      } = await loadFixture(setup);
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );

      // Submit tickets from all operators and finalize
      await registry.connect(operator1).submitTicket(0, 1);
      await registry.connect(operator2).submitTicket(0, 1);
      await registry.connect(operator3).submitTicket(0, 1);
      await finalizeCommitteeAfterWindow(registry, 0);

      await registry.publishCommittee(
        0,
        dataHash,
        encodeMockDkgProof(dataHash),
        "0x01",
      );

      const maxLength = await registry.MAX_COMMITTEE_PUBLIC_KEY_BYTES();
      await expect(registry.publishCommitteePublicKey(0, "0x"))
        .to.be.revertedWithCustomError(registry, "InvalidPublicKeyLength")
        .withArgs(0, maxLength);
      const oversizedKey = ethers.hexlify(
        new Uint8Array(Number(maxLength) + 1),
      );
      await expect(registry.publishCommitteePublicKey(0, oversizedKey))
        .to.be.revertedWithCustomError(registry, "InvalidPublicKeyLength")
        .withArgs(maxLength + 1n, maxLength);

      await expect(
        registry.connect(notTheOwner).publishCommitteePublicKey(0, "0xdead"),
      )
        .to.emit(registry, "CommitteePublished")
        .withArgs(
          0,
          [
            await operator3.getAddress(),
            await operator1.getAddress(),
            await operator2.getAddress(),
          ],
          "0xdead",
          dataHash,
          "0x",
        );

      await expect(registry.publishCommitteePublicKey(0, data))
        .to.emit(registry, "CommitteePublished")
        .withArgs(
          0,
          [
            await operator3.getAddress(),
            await operator1.getAddress(),
            await operator2.getAddress(),
          ],
          data,
          dataHash,
          "0x",
        );
    });
  });

  describe("getActiveCommitteeNodes()", function () {
    it("does not grant membership to provisional candidates", async function () {
      const { registry, operator1, request } = await loadFixture(setup);
      await request();

      await registry.connect(operator1).submitTicket(0, 1);
      const operator = await operator1.getAddress();

      expect(await registry.isCommitteeMember(0, operator)).to.equal(false);
      const [nodes] = await registry.getActiveCommitteeNodes(0);
      expect(nodes).to.deep.equal([]);
    });

    it("returns active committee nodes with their scores", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
        operator1,
        operator2,
        operator3,
      } = await loadFixture(setup);
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );

      await registry.connect(operator1).submitTicket(0, 1);
      await registry.connect(operator2).submitTicket(0, 1);
      await registry.connect(operator3).submitTicket(0, 1);
      await finalizeCommitteeAfterWindow(registry, 0);

      const finalizedEvents = await registry.queryFilter(
        registry.filters.SortitionCommitteeFinalized(0),
      );
      expect(finalizedEvents.length).to.equal(1);

      const finalizedEvent = finalizedEvents[0];
      const [activeNodes, activeScores] =
        await registry.getActiveCommitteeNodes(0);

      expect(activeNodes).to.deep.equal(finalizedEvent.args.committee);
      expect(activeScores).to.deep.equal(finalizedEvent.args.scores);
      for (const node of activeNodes) {
        expect(await registry.isCommitteeMemberActive(0, node)).to.equal(true);
      }
    });
  });

  describe("addCiphernode()", function () {
    it("reverts if the caller is not the owner", async function () {
      const { registry, notTheOwner } = await loadFixture(setup);
      await expect(
        registry.connect(notTheOwner).addCiphernode(AddressTwo),
      ).to.be.revertedWithCustomError(registry, "NotOwnerOrBondingRegistry");
    });
    it("adds the ciphernode to the registry", async function () {
      const { registry } = await loadFixture(setup);
      expect(await registry.addCiphernode(AddressTwo));
      expect(await registry.isEnabled(AddressTwo)).to.be.true;
    });
    it("increments numCiphernodes", async function () {
      const { registry } = await loadFixture(setup);
      const numCiphernodes = await registry.numCiphernodes();
      expect(await registry.addCiphernode(AddressTwo));
      expect(await registry.numCiphernodes()).to.equal(
        numCiphernodes + BigInt(1),
      );
    });
    it("emits a CiphernodeAdded event", async function () {
      const { registry } = await loadFixture(setup);
      const treeSize = await registry.treeSize();
      const numCiphernodes = await registry.numCiphernodes();
      await expect(await registry.addCiphernode(AddressTwo))
        .to.emit(registry, "CiphernodeAdded")
        .withArgs(
          AddressTwo,
          treeSize,
          numCiphernodes + BigInt(1),
          treeSize + BigInt(1),
        );
    });
  });

  describe("removeCiphernode()", function () {
    it("reverts if the caller is not the owner", async function () {
      const { registry, notTheOwner } = await loadFixture(setup);
      await expect(
        registry.connect(notTheOwner).removeCiphernode(AddressOne),
      ).to.be.revertedWithCustomError(registry, "NotOwnerOrBondingRegistry");
    });
    it("removes the ciphernode from the registry", async function () {
      const { registry, operator1 } = await loadFixture(setup);
      const operator1Address = await operator1.getAddress();
      const rootBefore = await registry.root();
      expect(await registry.isEnabled(operator1Address)).to.be.true;
      await registry.removeCiphernode(operator1Address);
      expect(await registry.isEnabled(operator1Address)).to.be.false;
      expect(await registry.root()).to.not.equal(rootBefore);
    });
    it("decrements numCiphernodes", async function () {
      const { registry, operator1 } = await loadFixture(setup);
      const operator1Address = await operator1.getAddress();
      const numCiphernodes = await registry.numCiphernodes();
      await registry.removeCiphernode(operator1Address);
      expect(await registry.numCiphernodes()).to.equal(
        numCiphernodes - BigInt(1),
      );
    });
    it("emits a CiphernodeRemoved event", async function () {
      const { registry, operator1 } = await loadFixture(setup);
      const operator1Address = await operator1.getAddress();
      const numCiphernodes = await registry.numCiphernodes();
      const size = await registry.treeSize();
      const index = await registry.ciphernodeTreeIndex(operator1Address);
      await expect(registry.removeCiphernode(operator1Address))
        .to.emit(registry, "CiphernodeRemoved")
        .withArgs(operator1Address, index, numCiphernodes - BigInt(1), size);
    });
  });

  describe("setInterfold()", function () {
    it("reverts if the caller is not the owner", async function () {
      const { registry, notTheOwner } = await loadFixture(setup);
      await expect(
        registry.connect(notTheOwner).setInterfold(AddressTwo),
      ).to.be.revertedWithCustomError(registry, "OwnableUnauthorizedAccount");
    });
    it("sets the interfold address", async function () {
      const { registry } = await loadFixture(setup);
      expect(await registry.setInterfold(AddressTwo));
      expect(await registry.interfold()).to.equal(AddressTwo);
    });
    it("emits an InterfoldSet event", async function () {
      const { registry } = await loadFixture(setup);
      await expect(await registry.setInterfold(AddressTwo))
        .to.emit(registry, "InterfoldSet")
        .withArgs(AddressTwo);
    });
  });

  describe("exit timing", function () {
    const ONE_DAY = 24 * 60 * 60;

    it("rejects a zero registry pointer in BondingRegistry", async function () {
      const { bondingRegistry } = await loadFixture(setup);
      await expect(
        bondingRegistry.setRegistry(ethers.ZeroAddress),
      ).to.be.revertedWithCustomError(bondingRegistry, "ZeroAddress");
    });

    it("keeps exit claims behind request-time committee deadlines", async function () {
      const {
        owner,
        operator1,
        registry,
        interfold,
        bondingRegistry,
        ticketToken,
        usdcToken,
        request,
      } = await loadFixture(setup);
      const oldSubmissionWindow = 2 * ONE_DAY;
      const oldExitDelay = 3 * ONE_DAY;

      await interfold.setTimeoutConfig({
        dkgWindow: ONE_DAY,
        computeWindow: 3 * ONE_DAY,
        decryptionWindow: ONE_DAY,
      });
      await bondingRegistry.setExitDelay(oldExitDelay);
      await registry.setSortitionSubmissionWindow(oldSubmissionWindow);
      await request();
      const oldDeadline = await registry.getCommitteeDeadline(0);

      await registry.setSortitionSubmissionWindow(10);
      await expect(
        bondingRegistry.setExitDelay(ONE_DAY),
      ).to.be.revertedWithCustomError(
        bondingRegistry,
        "ExitDelayMustExceedSortitionWindow",
      );

      await networkHelpers.time.increaseTo(oldDeadline + 1n);
      await bondingRegistry.setExitDelay(ONE_DAY);

      const operatorAddress = await operator1.getAddress();
      const exitAmount = ethers.parseUnits("1", 6);
      await bondingRegistry
        .connect(owner)
        .removeTicketBalanceFor(operatorAddress, exitAmount);
      await networkHelpers.time.increase(ONE_DAY + 1);

      const ownerAddress = await owner.getAddress();
      const balanceBefore = await usdcToken.balanceOf(ownerAddress);
      await bondingRegistry
        .connect(owner)
        .claimExitsFor(operatorAddress, exitAmount, 0);
      expect(await usdcToken.balanceOf(ownerAddress)).to.equal(
        balanceBefore + exitAmount,
      );
      expect(await ticketToken.balanceOf(operatorAddress)).to.be.gt(0);
      await expect(
        registry.connect(operator1).submitTicket(0, 1),
      ).to.be.revertedWithCustomError(registry, "CommitteeDeadlineReached");
    });
  });

  describe("committeePublicKey()", function () {
    it("returns the public key of the committee for the given e3Id", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
        operator1,
        operator2,
        operator3,
      } = await loadFixture(setup);
      const e3Id = 0;
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );

      await registry.connect(operator1).submitTicket(e3Id, 1);
      await registry.connect(operator2).submitTicket(e3Id, 1);
      await registry.connect(operator3).submitTicket(e3Id, 1);
      await finalizeCommitteeAfterWindow(registry, e3Id);

      await registry.publishCommittee(
        e3Id,
        dataHash,
        encodeMockDkgProof(dataHash),
        "0x01",
      );
      expect(await registry.committeePublicKey(e3Id)).to.equal(dataHash);
    });
    it("reverts if the committee has not been published", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      } = await loadFixture(setup);
      const e3Id = 0;
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );
      await expect(
        registry.committeePublicKey(e3Id),
      ).to.be.revertedWithCustomError(registry, "CommitteeNotPublished");
    });
  });

  describe("isCiphernodeEligible()", function () {
    it("returns true if the ciphernode is in the registry", async function () {
      const { registry, operator1 } = await loadFixture(setup);
      expect(await registry.isEnabled(await operator1.getAddress())).to.be.true;
    });
    it("returns false if the ciphernode is not in the registry", async function () {
      const { registry } = await loadFixture(setup);
      expect(await registry.isCiphernodeEligible(AddressTwo)).to.be.false;
    });
  });

  describe("isEnabled()", function () {
    it("returns true if the ciphernode is currently enabled", async function () {
      const { registry, operator1 } = await loadFixture(setup);
      expect(await registry.isEnabled(await operator1.getAddress())).to.be.true;
    });
    it("returns false if the ciphernode is not currently enabled", async function () {
      const { registry } = await loadFixture(setup);
      expect(await registry.isEnabled(AddressTwo)).to.be.false;
    });
  });

  describe("root()", function () {
    it("returns a non-zero root when ciphernodes are registered", async function () {
      const { registry } = await loadFixture(setup);
      expect(await registry.root()).to.not.equal(0);
    });
  });

  describe("rootAt()", function () {
    it("returns the root of the ciphernode registry merkle tree at the given e3Id", async function () {
      const {
        registry,
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      } = await loadFixture(setup);
      const e3Id = 0;
      const rootBeforeRequest = await registry.root();
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );
      expect(await registry.rootAt(e3Id)).to.equal(rootBeforeRequest);
    });
  });

  describe("treeSize()", function () {
    it("returns the size of the ciphernode registry merkle tree", async function () {
      const { registry } = await loadFixture(setup);
      // Three operators registered in setup
      expect(await registry.treeSize()).to.equal(3);
    });
  });
});
