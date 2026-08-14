// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";
import fs from "fs";
import { network } from "hardhat";

import { deployProtocolContracts } from "../../scripts/protocol/deployContracts";
import { buildSafeTransactions } from "../../scripts/protocol/transactions";
import type { ProtocolConfigFile } from "../../scripts/protocol/types";
import { BondingRegistry__factory as BondingRegistryFactory } from "../../types";

const { ethers } = await network.connect();

describe("Protocol deployment", function () {
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
    const programFactory = await ethers.getContractFactory("MockE3Program");
    const program = await programFactory.deploy();
    await program.waitForDeployment();
    // FOLD has to be a real votes token: the deployment builds `BondedVotes` against it, and that
    // constructor compares the token's ERC-6372 clock with the bonded history's.
    const foldFactory = await ethers.getContractFactory("MockVotesToken");
    const fold = await foldFactory.deploy();
    await fold.waitForDeployment();

    const config = JSON.parse(
      fs.readFileSync(
        new URL(
          "../../deploy/protocol/example.protocol.config.json",
          import.meta.url,
        ),
        "utf8",
      ),
    ) as ProtocolConfigFile;
    config.safe = await safe.getAddress();
    config.fold = await fold.getAddress();
    config.bondingRegistryProxy = await bondingProxy.getAddress();
    config.bondingRegistryProxyAdmin = await bondingProxyAdmin.getAddress();
    config.feeToken = await feeToken.getAddress();
    config.ticketUnderlyingToken = await ticketUnderlyingToken.getAddress();
    config.protocolTreasury = await safe.getAddress();
    config.slashedFundsTreasury = await safe.getAddress();
    config.interfold.pricing.protocolTreasury = await safe.getAddress();
    config.e3Programs = [await program.getAddress()];

    const result = await deployProtocolContracts(ethers, operator, config);
    const ticket = await ethers.getContractAt(
      "InterfoldTicketToken",
      result.contracts.ticketToken,
    );
    const interfold = await ethers.getContractAt(
      "Interfold",
      result.contracts.interfold,
    );

    expect(await ticket.underlying()).to.equal(
      await ticketUnderlyingToken.getAddress(),
    );
    expect(await interfold.feeToken()).to.equal(await feeToken.getAddress());

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
