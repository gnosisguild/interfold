// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { expect } from "chai";
import { network } from "hardhat";

const { ethers } = await network.connect();

describe("CommitteeHashLib", function () {
  const nodes = [
    "0x0000000000000000000000001234567890abcdef",
    "0x1111111111111111111111111234567890abcdef",
    "0xabcdefabcdefabcdefabcdef0123456789abcdef",
  ];
  const expected =
    "0x47416ae429c0010f46c2f61a7fff4ed80384e64a6b1709b84416f27790ec5f20";

  it("hashes every byte of each ordered address", async function () {
    const factory = await ethers.getContractFactory("CommitteeHashHarness");
    const harness = await factory.deploy();

    expect(await harness.hash(nodes)).to.equal(expected);
    expect(await harness.hash([nodes[0]])).to.not.equal(
      await harness.hash([nodes[1]]),
    );
  });
});
