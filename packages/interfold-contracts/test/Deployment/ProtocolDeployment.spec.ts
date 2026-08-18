// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";
import { ethers as ethersLib } from "ethers";
import fs from "fs";
import { network } from "hardhat";
import os from "os";
import path from "path";

import { deployProtocolContracts } from "../../scripts/protocol/deployContracts";
import {
  aragonAdminSafeBatch,
  aragonAdminSafeTransactions,
  safeTx,
} from "../../scripts/protocol/safe";
import { buildSafeTransactions } from "../../scripts/protocol/transactions";
import type { ProtocolConfigFile } from "../../scripts/protocol/types";
import { loadConfig } from "../../scripts/protocol/values";
import { BondingRegistry__factory as BondingRegistryFactory } from "../../types";

const { ethers } = await network.connect();

describe("Protocol deployment", function () {
  it("wraps DAO wiring actions in one Aragon Admin Safe transaction", async function () {
    const [adminPlugin, proposerSafe, targetA, targetB] =
      await ethers.getSigners();
    const config = {
      name: "mainnet-protocol",
      chainId: 1,
      protocolOwner: "0x652a31c669f9AB37f6040f279139a75D04F2679e",
      governance: {
        adminPlugin: await adminPlugin.getAddress(),
        proposerSafe: await proposerSafe.getAddress(),
        proposalMetadata: "0x",
      },
    } as ProtocolConfigFile;
    const actions = [
      safeTx(await targetA.getAddress(), "0x12345678"),
      safeTx(await targetB.getAddress(), "0xabcdef01"),
    ];

    const batch = aragonAdminSafeBatch(config, actions);
    expect(batch.meta.createdFromSafeAddress).to.equal(
      await proposerSafe.getAddress(),
    );
    expect(batch.transactions).to.have.lengthOf(1);

    const [wrapper] = aragonAdminSafeTransactions(config, actions);
    expect(wrapper.to).to.equal(await adminPlugin.getAddress());

    const adminInterface = new ethersLib.Interface([
      "function executeProposal(bytes metadata,tuple(address to,uint256 value,bytes data)[] actions,uint256 allowFailureMap)",
    ]);
    const decoded = adminInterface.decodeFunctionData(
      "executeProposal",
      wrapper.data,
    );

    expect(decoded.metadata).to.equal("0x");
    expect(decoded.allowFailureMap).to.equal(0n);
    expect(decoded.actions).to.have.lengthOf(actions.length);
    for (const [index, action] of actions.entries()) {
      expect(decoded.actions[index].to).to.equal(action.to);
      expect(decoded.actions[index].value).to.equal(BigInt(action.value));
      expect(decoded.actions[index].data).to.equal(action.data);
    }
  });

  it("rejects non-call actions in Aragon Admin Safe wrappers", async function () {
    const [adminPlugin, proposerSafe, target] = await ethers.getSigners();
    const config = {
      name: "mainnet-protocol",
      chainId: 1,
      protocolOwner: "0x652a31c669f9AB37f6040f279139a75D04F2679e",
      governance: {
        adminPlugin: await adminPlugin.getAddress(),
        proposerSafe: await proposerSafe.getAddress(),
      },
    } as ProtocolConfigFile;
    const tx = safeTx(await target.getAddress(), "0x12345678");
    tx.operation = 1;

    expect(() => aragonAdminSafeTransactions(config, [tx])).to.throw(
      "Governance transaction 1 is not a CALL operation",
    );
  });

  it("rejects a zero protocol owner and accepts a missing-owner override", function () {
    const source = new URL(
      "../../deploy/protocol/example.protocol.config.json",
      import.meta.url,
    );
    const config = JSON.parse(fs.readFileSync(source, "utf8"));
    const tempDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "interfold-protocol-config-"),
    );
    const configFile = path.join(tempDir, "protocol.json");
    const previousOwner = process.env.PROTOCOL_OWNER;

    try {
      fs.writeFileSync(configFile, JSON.stringify(config));
      expect(() => loadConfig(configFile)).to.throw(
        "protocolOwner must not be the zero address",
      );

      delete config.protocolOwner;
      fs.writeFileSync(configFile, JSON.stringify(config));
      process.env.PROTOCOL_OWNER = "0x0000000000000000000000000000000000000001";
      expect(loadConfig(configFile).protocolOwner).to.equal(
        "0x0000000000000000000000000000000000000001",
      );
    } finally {
      if (previousOwner === undefined) delete process.env.PROTOCOL_OWNER;
      else process.env.PROTOCOL_OWNER = previousOwner;
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it("rejects external wiring for the deployed MockE3Program", function () {
    const source = new URL(
      "../../deploy/protocol/example.protocol.config.json",
      import.meta.url,
    );
    const config = JSON.parse(fs.readFileSync(source, "utf8"));
    const tempDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "interfold-mock-program-config-"),
    );
    const configFile = path.join(tempDir, "protocol.json");

    try {
      config.protocolOwner = "0x0000000000000000000000000000000000000001";
      config.e3Programs = ["0x0000000000000000000000000000000000000002"];
      fs.writeFileSync(configFile, JSON.stringify(config));
      expect(() => loadConfig(configFile)).to.throw(
        "e3Programs[0] must be the zero address when deployMockE3Program is true",
      );

      config.e3Programs = [ethersLib.ZeroAddress];
      config.bindInitialE3Program = true;
      config.ciphertextVerifier = "0x0000000000000000000000000000000000000002";
      fs.writeFileSync(configFile, JSON.stringify(config));
      expect(() => loadConfig(configFile)).to.throw(
        "bindInitialE3Program must be false when deployMockE3Program is true",
      );
    } finally {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it("uses separate fee and ticket collateral tokens", async function () {
    const [operator, safe, bondingProxy, bondingProxyAdmin] =
      await ethers.getSigners();
    const tokenFactory = await ethers.getContractFactory(
      "MockFeeOnTransferToken",
    );
    // `deploy()` resolves once the transaction is sent, not once it is mined,
    // and `getAddress()` returns the computed address either way. The addresses
    // below are fed into further deployments, and Interfold rejects a program
    // address with no runtime code, so each deployment has to land first.
    const feeToken = await tokenFactory.deploy(0);
    await feeToken.waitForDeployment();
    const ticketUnderlyingToken = await tokenFactory.deploy(0);
    await ticketUnderlyingToken.waitForDeployment();
    // FOLD has to be a real votes token: the deployment builds `BondedVotes` against it, and that
    // constructor compares the token's ERC-6372 clock with the bonded history's.
    const foldFactory = await ethers.getContractFactory("MockVotesToken");
    const fold = await foldFactory.deploy();
    await fold.waitForDeployment();
    const decryptionVerifier = await ethers.deployContract(
      "MockDecryptionVerifier",
    );
    await decryptionVerifier.waitForDeployment();
    const pkVerifier = await ethers.deployContract("MockPkVerifier");
    await pkVerifier.waitForDeployment();
    const dkgFoldAttestationVerifier = await ethers.deployContract(
      "DkgFoldAttestationVerifier",
    );
    await dkgFoldAttestationVerifier.waitForDeployment();

    const config = JSON.parse(
      fs.readFileSync(
        new URL(
          "../../deploy/protocol/example.protocol.config.json",
          import.meta.url,
        ),
        "utf8",
      ),
    ) as ProtocolConfigFile;
    config.protocolOwner = await safe.getAddress();
    config.safe = await safe.getAddress();
    config.fold = await fold.getAddress();
    config.bondingRegistryProxy = await bondingProxy.getAddress();
    config.bondingRegistryProxyAdmin = await bondingProxyAdmin.getAddress();
    config.feeToken = await feeToken.getAddress();
    config.ticketUnderlyingToken = await ticketUnderlyingToken.getAddress();
    config.protocolTreasury = await safe.getAddress();
    config.slashedFundsTreasury = await safe.getAddress();
    config.interfold.pricing.protocolTreasury = await safe.getAddress();
    config.verifiers = {
      deploy: false,
      decryptionVerifier: await decryptionVerifier.getAddress(),
      pkVerifier: await pkVerifier.getAddress(),
      dkgFoldAttestationVerifier: await dkgFoldAttestationVerifier.getAddress(),
    };

    const result = await deployProtocolContracts(ethers, operator, config);
    const ticket = await ethers.getContractAt(
      "InterfoldTicketToken",
      result.contracts.ticketToken,
    );
    const interfold = await ethers.getContractAt(
      "Interfold",
      result.contracts.interfold,
    );
    const program = await ethers.getContractAt(
      "MockE3Program",
      result.contracts.initialE3Program,
    );

    expect(await ticket.underlying()).to.equal(
      await ticketUnderlyingToken.getAddress(),
    );
    expect(await interfold.feeToken()).to.equal(await feeToken.getAddress());
    expect(
      await interfold.e3Programs(result.contracts.initialE3Program),
    ).to.equal(true);
    expect(await program.ENCRYPTION_SCHEME_ID()).to.equal(
      ethersLib.id("fhe.rs:BFV"),
    );
    expect(result.contracts.decryptionVerifier).to.equal(
      await decryptionVerifier.getAddress(),
    );
    expect(result.contracts.pkVerifier).to.equal(await pkVerifier.getAddress());
    expect(result.contracts.dkgFoldAttestationVerifier).to.equal(
      await dkgFoldAttestationVerifier.getAddress(),
    );
    for (const verifier of [
      result.contracts.decryptionVerifier,
      result.contracts.pkVerifier,
      result.contracts.dkgFoldAttestationVerifier,
    ]) {
      expect(verifier).to.match(/^0x[0-9a-fA-F]{40}$/);
      expect(await ethers.provider.getCode(verifier as string)).to.not.equal(
        "0x",
      );
    }

    // Bonded voting has to be deployed and wired by the deployment itself. Shipping the registry
    // without it leaves the feature silently disabled: the sync is a no-op while unconfigured, so
    // every operator would read as holding no bonded voting power.
    const checkpoints = await ethers.getContractAt(
      "BondedCheckpoints",
      result.contracts.bondedCheckpoints,
    );
    // Bound to the proxy, not the implementation: the proxy is what calls `sync`.
    expect(await checkpoints.registry()).to.equal(config.bondingRegistryProxy);

    // `BondedVotes` is deliberately absent here. Its constructor asks the registry which token it
    // bonds, and the registry is only initialized by the Safe batch this step writes — so it is
    // deployed by `--action activate-voting` afterwards.
    expect(result.contracts).to.not.have.property("bondedVotes");

    // The batch must carry the call that attaches the history, or none of the above is reachable.
    const txs = buildSafeTransactions(
      config,
      result.contracts,
      result.interfaces,
    );
    const selector = BondingRegistryFactory.createInterface().getFunction(
      "setBondedCheckpoints",
    ).selector;
    const attach = txs.filter(
      (tx) =>
        tx.to.toLowerCase() === config.bondingRegistryProxy.toLowerCase() &&
        tx.data.startsWith(selector),
    );
    expect(attach).to.have.lengthOf(1);
    expect(attach[0].data.toLowerCase()).to.contain(
      result.contracts.bondedCheckpoints.slice(2).toLowerCase(),
    );
  });
});
