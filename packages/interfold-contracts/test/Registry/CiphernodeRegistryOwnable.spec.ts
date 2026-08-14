// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";
import type { Signer } from "ethers";

import { CiphernodeRegistryOwnable__factory as CiphernodeRegistryFactory } from "../../types";
import {
  ACTIVE_CRYPTO_CONFIG_ID,
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
const SORTITION_SUBMISSION_WINDOW = 60;

describe("CiphernodeRegistryOwnable", function () {
  let firstE3Id: bigint;
  async function finalizeCommitteeAfterWindow(
    registry: any,
    e3Id: number | bigint,
  ): Promise<void> {
    await networkHelpers.time.increase(SORTITION_SUBMISSION_WINDOW + 1);
    await registry.finalizeCommittee(e3Id);
  }

  async function setup() {
    const sys = await deployInterfoldSystem({
      submissionWindow: SORTITION_SUBMISSION_WINDOW,
      committeeThresholds: [[0, [2, 3]]],
    });
    firstE3Id = await sys.interfold.nexte3Id();
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
    mineEntropyBlock: boolean = true,
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
      expectedFeeToken: await usdcToken.getAddress(),
      expectedCryptoConfigId: ACTIVE_CRYPTO_CONFIG_ID,
      maxFee: ethers.MaxUint256,
    };

    const fee = await interfold.getE3Quote(requestParams);
    const tokenContract = signer ? usdcToken.connect(signer) : usdcToken;
    const interfoldContract = signer ? interfold.connect(signer) : interfold;

    await tokenContract.approve(await interfold.getAddress(), fee);
    const tx = await interfoldContract.request(requestParams);
    if (mineEntropyBlock) await networkHelpers.mine(1);
    return tx;
  }

  describe("constructor / initialize()", function () {
    it("correctly sets `_owner` and `interfold` ", async function () {
      const poseidonFactory = await ethers.getContractFactory("PoseidonT3");
      const poseidonDeployment = await poseidonFactory.deploy();
      await poseidonDeployment.waitForDeployment();
      const poseidonAddress = await poseidonDeployment.getAddress();
      const sortitionFactory = await ethers.getContractFactory(
        "RegistrySortitionLib",
      );
      const sortitionDeployment = await sortitionFactory.deploy();
      await sortitionDeployment.waitForDeployment();
      const sortitionAddress = await sortitionDeployment.getAddress();
      const [deployer] = await ethers.getSigners();
      if (!deployer) throw new Error("Bad getSigners() output");

      const ciphernodeRegistryFactory = await ethers.getContractFactory(
        "CiphernodeRegistryOwnable",
        {
          libraries: {
            PoseidonT3: poseidonAddress,
            RegistrySortitionLib: sortitionAddress,
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
      expect(await registry.rootAt(firstE3Id)).to.equal(await registry.root());
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
      expect(await registry.rootAt(firstE3Id)).to.equal(await registry.root());
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

    it("reveals the committee seed only after the committed future block", async function () {
      const {
        registry,
        interfold,
        operator1,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      } = await loadFixture(setup);

      const tx = await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
        undefined,
        false,
      );
      const receipt = await tx.wait();
      if (!receipt) throw new Error("request receipt missing");

      const entropyBlock = await registry.sortitionEntropyBlocks(firstE3Id);
      expect(entropyBlock).to.equal(receipt.blockNumber + 1);
      expect(await registry.sortitionSeed(firstE3Id)).to.deep.equal([
        false,
        0n,
      ]);

      await networkHelpers.mine(2);
      const entropy = await ethers.provider.getBlock(Number(entropyBlock));
      if (!entropy) throw new Error("entropy block missing");
      const expectedSeed = BigInt(
        ethers.keccak256(
          ethers.AbiCoder.defaultAbiCoder().encode(
            ["bytes32", "uint256"],
            [entropy.hash, firstE3Id],
          ),
        ),
      );

      expect(await registry.sortitionSeed(firstE3Id)).to.deep.equal([
        true,
        expectedSeed,
      ]);

      const expectedScore = BigInt(
        ethers.keccak256(
          ethers.solidityPacked(
            ["address", "uint256", "uint256", "uint256"],
            [await operator1.getAddress(), 1, firstE3Id, expectedSeed],
          ),
        ),
      );
      await expect(registry.connect(operator1).submitTicket(firstE3Id, 1))
        .to.emit(registry, "TicketSubmitted")
        .withArgs(firstE3Id, await operator1.getAddress(), 1, expectedScore);
    });

    it("uses retained entropy through the maximum submission window", async function () {
      const {
        registry,
        interfold,
        operator1,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      } = await loadFixture(setup);

      const maximumWindow = await registry.MAX_SORTITION_SUBMISSION_WINDOW();
      expect(maximumWindow).to.equal(24n * 60n * 60n);
      await registry.setSortitionSubmissionWindow(maximumWindow);
      await makeRequest(
        interfold,
        usdcToken,
        mockE3Program,
        mockDecryptionVerifier,
      );
      const entropyBlock = await registry.sortitionEntropyBlocks(firstE3Id);
      const entropy = await ethers.provider.getBlock(Number(entropyBlock));
      if (!entropy?.hash) throw new Error("entropy block hash missing");
      const entropyHash = entropy.hash;

      await networkHelpers.mine(257);
      expect(await registry.sortitionSeed(firstE3Id)).to.deep.equal([
        false,
        0n,
      ]);

      const runtimeCode = `0x7f${entropyHash.slice(2)}60005260206000f3`;
      await ethers.provider.send("hardhat_setCode", [
        await registry.BLOCKHASH_HISTORY(),
        runtimeCode,
      ]);

      const expectedSeed = BigInt(
        ethers.keccak256(
          ethers.AbiCoder.defaultAbiCoder().encode(
            ["bytes32", "uint256"],
            [entropyHash, firstE3Id],
          ),
        ),
      );
      expect(await registry.sortitionSeed(firstE3Id)).to.deep.equal([
        true,
        expectedSeed,
      ]);
      const expectedScore = BigInt(
        ethers.keccak256(
          ethers.solidityPacked(
            ["address", "uint256", "uint256", "uint256"],
            [await operator1.getAddress(), 1, firstE3Id, expectedSeed],
          ),
        ),
      );
      const deadline = await registry.getCommitteeDeadline(firstE3Id);
      await networkHelpers.time.setNextBlockTimestamp(deadline);
      await expect(registry.connect(operator1).submitTicket(firstE3Id, 1))
        .to.emit(registry, "TicketSubmitted")
        .withArgs(firstE3Id, await operator1.getAddress(), 1, expectedScore);
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
      expect(await registry.rootAt(firstE3Id)).to.not.equal(0);
    });

    it("allows one ticket ID across concurrent E3 requests", async function () {
      const { registry, operator1, request } = await loadFixture(setup);

      for (let offset = 0n; offset < 2n; offset++) {
        await request();
        await registry.connect(operator1).submitTicket(firstE3Id + offset, 1);
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

      await registry.connect(owner).setSortitionSubmissionWindow(60);
      await request();
      expect(await registry.sortitionTicketPrices(firstE3Id)).to.equal(
        TICKET_PRICE,
      );

      await setBondingAssetConfig(bondingRegistry, {
        ticketPrice: TICKET_PRICE * 2n,
      });
      await bondingRegistry.refreshOperatorStatuses([
        await operator1.getAddress(),
        await operator2.getAddress(),
        await operator3.getAddress(),
      ]);
      await registry.connect(operator1).submitTicket(firstE3Id, 10);

      await setBondingAssetConfig(bondingRegistry, {
        ticketPrice: TICKET_PRICE / 2n,
      });
      await bondingRegistry.refreshOperatorStatuses([
        await operator1.getAddress(),
        await operator2.getAddress(),
        await operator3.getAddress(),
      ]);

      await expect(
        registry.connect(operator2).submitTicket.staticCall(firstE3Id, 11),
      ).to.be.revertedWithCustomError(registry, "InvalidTicketNumber");
      await registry.connect(operator2).submitTicket(firstE3Id, 10);
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
        registry.connect(lateOperator).submitTicket(firstE3Id, 1),
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
      expect(await registry.rootAt(firstE3Id)).to.equal(await registry.root());
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
      const e3Id = await interfold.nexte3Id();
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
        registry.connect(operator1).submitTicket(e3Id, 1),
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
      await registry.connect(operator1).submitTicket(e3Id + 1n, 1);
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

      expect(await registry.dkgFoldAttestationVerifierFor(firstE3Id)).to.equal(
        oldVerifier,
      );
      expect(
        await registry.dkgFoldAttestationVerifierFor(firstE3Id + 1n),
      ).to.equal(await newVerifier.getAddress());
      const contextEvents = await registry.queryFilter(
        registry.filters.DkgFoldAttestationContextEstablished(),
      );
      expect(contextEvents.map((event) => event.args.e3Id)).to.deep.equal([
        firstE3Id,
        firstE3Id + 1n,
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
      await registry.connect(operator1).submitTicket(firstE3Id, 1);
      await registry.connect(operator2).submitTicket(firstE3Id, 1);
      await registry.connect(operator3).submitTicket(firstE3Id, 1);
      await finalizeCommitteeAfterWindow(registry, firstE3Id);

      await expect(
        registry.publishCommittee(firstE3Id, dataHash, "0x", "0x"),
      ).to.be.revertedWithCustomError(registry, "DkgProofRequired");
      await expect(
        registry.publishCommittee(
          firstE3Id,
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
      await registry.connect(operator1).submitTicket(firstE3Id, 1);
      await registry.connect(operator2).submitTicket(firstE3Id, 1);
      await registry.connect(operator3).submitTicket(firstE3Id, 1);
      await finalizeCommitteeAfterWindow(registry, firstE3Id);

      const falseProof = ethers.AbiCoder.defaultAbiCoder().encode(
        ["bytes", "bytes32[]"],
        ["0xfafafafa", [dataHash]],
      );
      await expect(
        registry.publishCommittee(firstE3Id, dataHash, falseProof, "0x01"),
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

      await registry.connect(operator1).submitTicket(firstE3Id, 1);
      await registry.connect(operator2).submitTicket(firstE3Id, 1);
      await registry.connect(operator3).submitTicket(firstE3Id, 1);
      await finalizeCommitteeAfterWindow(registry, firstE3Id);

      await expect(
        registry
          .connect(notTheOwner)
          .publishCommittee(
            firstE3Id,
            dataHash,
            encodeMockDkgProof(dataHash),
            "0x01",
          ),
      )
        .to.emit(registry, "CommitteeProofPublished")
        .withArgs(
          firstE3Id,
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

      await registry.connect(operator1).submitTicket(firstE3Id, 1);
      await registry.connect(operator2).submitTicket(firstE3Id, 1);
      await registry.connect(operator3).submitTicket(firstE3Id, 1);
      await finalizeCommitteeAfterWindow(registry, firstE3Id);

      await registry.publishCommittee(
        firstE3Id,
        dataHash,
        encodeMockDkgProof(dataHash),
        "0x01",
      );
      expect(await registry.committeePublicKey(firstE3Id)).to.equal(dataHash);
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
      await registry.connect(operator1).submitTicket(firstE3Id, 1);
      await registry.connect(operator2).submitTicket(firstE3Id, 1);
      await registry.connect(operator3).submitTicket(firstE3Id, 1);
      await finalizeCommitteeAfterWindow(registry, firstE3Id);

      await registry.publishCommittee(
        firstE3Id,
        dataHash,
        encodeMockDkgProof(dataHash),
        "0x01",
      );

      const maxLength = await registry.MAX_COMMITTEE_PUBLIC_KEY_BYTES();
      await expect(registry.publishCommitteePublicKey(firstE3Id, "0x"))
        .to.be.revertedWithCustomError(registry, "InvalidPublicKeyLength")
        .withArgs(0, maxLength);
      const oversizedKey = ethers.hexlify(
        new Uint8Array(Number(maxLength) + 1),
      );
      await expect(registry.publishCommitteePublicKey(firstE3Id, oversizedKey))
        .to.be.revertedWithCustomError(registry, "InvalidPublicKeyLength")
        .withArgs(maxLength + 1n, maxLength);

      await expect(
        registry
          .connect(notTheOwner)
          .publishCommitteePublicKey(firstE3Id, "0xdead"),
      )
        .to.emit(registry, "CommitteePublished")
        .withArgs(
          firstE3Id,
          [
            await operator3.getAddress(),
            await operator1.getAddress(),
            await operator2.getAddress(),
          ],
          "0xdead",
          dataHash,
          "0x",
        );

      await expect(registry.publishCommitteePublicKey(firstE3Id, data))
        .to.emit(registry, "CommitteePublished")
        .withArgs(
          firstE3Id,
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

      await registry.connect(operator1).submitTicket(firstE3Id, 1);
      const operator = await operator1.getAddress();

      expect(await registry.isCommitteeMember(firstE3Id, operator)).to.equal(
        false,
      );
      const [nodes] = await registry.getActiveCommitteeNodes(firstE3Id);
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

      await registry.connect(operator1).submitTicket(firstE3Id, 1);
      await registry.connect(operator2).submitTicket(firstE3Id, 1);
      await registry.connect(operator3).submitTicket(firstE3Id, 1);
      await finalizeCommitteeAfterWindow(registry, firstE3Id);

      const finalizedEvents = await registry.queryFilter(
        registry.filters.SortitionCommitteeFinalized(firstE3Id),
      );
      expect(finalizedEvents.length).to.equal(1);

      const finalizedEvent = finalizedEvents[0];
      const [activeNodes, activeScores] =
        await registry.getActiveCommitteeNodes(firstE3Id);

      expect(activeNodes).to.deep.equal(finalizedEvent.args.committee);
      expect(activeScores).to.deep.equal(finalizedEvent.args.scores);
      for (const node of activeNodes) {
        expect(
          await registry.isCommitteeMemberActive(firstE3Id, node),
        ).to.equal(true);
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
    it("reuses a removed tree slot before growing the tree", async function () {
      const { registry, operator3 } = await loadFixture(setup);
      const removed = await operator3.getAddress();
      const removedIndex = await registry.ciphernodeTreeIndex(removed);
      expect(removedIndex).to.be.gt(0);
      const treeSize = await registry.treeSize();

      await registry.removeCiphernode(removed);
      await registry.addCiphernode(AddressTwo);

      expect(await registry.ciphernodeTreeIndex(AddressTwo)).to.equal(
        removedIndex,
      );
      expect(await registry.treeSize()).to.equal(treeSize);
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
      const { ciphernodeRegistry: registry } = await deployInterfoldSystem({
        setupOperators: 0,
      });
      expect(await registry.setInterfold(AddressTwo));
      expect(await registry.interfold()).to.equal(AddressTwo);
    });
    it("emits an InterfoldSet event", async function () {
      const { ciphernodeRegistry: registry } = await deployInterfoldSystem({
        setupOperators: 0,
      });
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
      const oldSubmissionWindow = ONE_DAY;
      const oldExitDelay = 2 * ONE_DAY;

      await interfold.setTimeoutConfig({
        dkgWindow: ONE_DAY,
        computeWindow: 3 * ONE_DAY,
        decryptionWindow: ONE_DAY,
      });
      await bondingRegistry.setExitDelay(oldExitDelay);
      await registry.setSortitionSubmissionWindow(oldSubmissionWindow);
      await request();
      const oldDeadline = await registry.getCommitteeDeadline(firstE3Id);

      await expect(
        bondingRegistry.setExitDelay(ONE_DAY),
      ).to.be.revertedWithCustomError(
        bondingRegistry,
        "ExitDelayMustExceedSortitionWindow",
      );

      await registry.setSortitionSubmissionWindow(60);
      await bondingRegistry.setExitDelay(ONE_DAY);

      const operatorAddress = await operator1.getAddress();
      const exitAmount = ethers.parseUnits("1", 6);
      await bondingRegistry
        .connect(owner)
        .removeTicketBalanceFor(operatorAddress, exitAmount);
      await networkHelpers.time.increase(ONE_DAY + 1);
      expect(BigInt(await networkHelpers.time.latest())).to.be.greaterThan(
        oldDeadline,
      );

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
        registry.connect(operator1).submitTicket(firstE3Id, 1),
      ).to.be.revertedWithCustomError(registry, "CommitteeDeadlineReached");
    });

    it("locks top candidates and releases a displaced candidate", async function () {
      const {
        owner,
        operator1,
        operator2,
        operator3,
        registry,
        bondingRegistry,
        licenseToken,
        ticketToken,
        usdcToken,
        request,
      } = await loadFixture(setup);
      const operator4 = (await ethers.getSigners())[5]!;
      await setupOperatorForSortition(
        operator4,
        owner,
        bondingRegistry,
        licenseToken,
        usdcToken,
        ticketToken,
        registry,
      );
      const candidates = [operator1, operator2, operator3, operator4];
      const exitAmount = ethers.parseUnits("1", 6);

      await bondingRegistry.setExitDelay(ONE_DAY);
      for (const candidate of candidates) {
        await bondingRegistry
          .connect(owner)
          .removeTicketBalanceFor(await candidate.getAddress(), exitAmount);
      }
      await networkHelpers.time.increase(ONE_DAY + 1);
      await request();
      await networkHelpers.mine(1);

      const [, seed] = await registry.sortitionSeed(firstE3Id);
      const ranked = await Promise.all(
        candidates.map(async (candidate) => ({
          candidate,
          score: BigInt(
            ethers.keccak256(
              ethers.solidityPacked(
                ["address", "uint256", "uint256", "uint256"],
                [await candidate.getAddress(), 1, firstE3Id, seed],
              ),
            ),
          ),
        })),
      );
      ranked.sort((left, right) =>
        left.score < right.score ? -1 : left.score > right.score ? 1 : 0,
      );
      const best = ranked[0]!.candidate;
      for (const entry of ranked.slice(1)) {
        await registry.connect(entry.candidate).submitTicket(firstE3Id, 1);
      }
      await registry.connect(best).submitTicket(firstE3Id, 1);

      await networkHelpers.time.increase(SORTITION_SUBMISSION_WINDOW + 1);
      await bondingRegistry.claimExitsFor(
        await ranked[3]!.candidate.getAddress(),
        exitAmount,
        0,
      );
      await expect(
        bondingRegistry.claimExitsFor(await best.getAddress(), exitAmount, 0),
      ).to.be.revertedWithCustomError(
        bondingRegistry,
        "OperatorInActiveCommittee",
      );
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
      const e3Id = firstE3Id;
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
      const e3Id = firstE3Id;
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
      const e3Id = firstE3Id;
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

describe("RegistrySortitionLib entropy", function () {
  const ARBSYS = "0x0000000000000000000000000000000000000064";
  const BLOCKHASH_HISTORY = "0x0000F90827F1C53a10cb7A02335B175320002935";

  async function setupEntropyHarness() {
    const factory = await ethers.getContractFactory("RegistrySortitionLib");
    const library = await factory.deploy();
    await library.waitForDeployment();
    return { library };
  }

  it("uses Arbitrum L2 block numbers and L2 block-hash history", async function () {
    const { library } = await loadFixture(setupEntropyHarness);

    await networkHelpers.mine(2);
    const currentBlock = await ethers.provider.getBlockNumber();
    const recentBlock = await ethers.provider.getBlock(currentBlock - 1);
    if (!recentBlock?.hash) throw new Error("recent block hash missing");

    const l2BlockNumber = currentBlock + 100;
    const encodedL2BlockNumber = ethers
      .zeroPadValue(ethers.toBeHex(l2BlockNumber), 32)
      .slice(2);
    await ethers.provider.send("hardhat_setCode", [
      ARBSYS,
      `0x7f${encodedL2BlockNumber}60005260206000f3`,
    ]);
    expect(await library.currentBlockNumber(42161)).to.equal(l2BlockNumber);
    expect(await library.currentBlockNumber(42170)).to.equal(l2BlockNumber);
    expect(await library.currentBlockNumber(421614)).to.equal(l2BlockNumber);

    const l2BlockHash = `0x${"11".repeat(32)}`;
    await ethers.provider.send("hardhat_setCode", [
      BLOCKHASH_HISTORY,
      `0x7f${l2BlockHash.slice(2)}60005260206000f3`,
    ]);

    expect(await library.entropyBlockHash(1, currentBlock - 1)).to.deep.equal([
      true,
      recentBlock.hash,
    ]);
    expect(
      await library.entropyBlockHash(42161, currentBlock - 1),
    ).to.deep.equal([true, l2BlockHash]);
  });
});
