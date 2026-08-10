// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";
import fs from "fs";
import { network } from "hardhat";

import { deployProtocolContracts } from "../../scripts/protocol/deployContracts";
import type { ProtocolConfigFile } from "../../scripts/protocol/types";

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
    config.fold = await operator.getAddress();
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
  });
});
