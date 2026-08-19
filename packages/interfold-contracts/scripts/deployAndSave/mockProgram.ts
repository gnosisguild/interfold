// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import type { HardhatRuntimeEnvironment } from "hardhat/types/hre";

import {
  MockE3ProgramHarness__factory as MockE3ProgramFactory,
  MockE3ProgramHarness,
} from "../../types";
import { getDeploymentChain, storeDeploymentArgs } from "../utils";

interface MockProgramArgs {
  hre: HardhatRuntimeEnvironment;
}

export const deployAndSaveMockProgram = async ({
  hre,
}: MockProgramArgs): Promise<{
  e3Program: MockE3ProgramHarness;
}> => {
  const { ethers } = await hre.network.connect();
  const [signer] = await ethers.getSigners();
  const chain = getDeploymentChain(hre);

  const e3ProgramFactory = await ethers.getContractFactory(
    "MockE3ProgramHarness",
  );
  const e3Program = await e3ProgramFactory.deploy();

  await e3Program.waitForDeployment();

  const e3ProgramAddress = await e3Program.getAddress();
  const blockNumber = await ethers.provider.getBlockNumber();

  storeDeploymentArgs(
    {
      blockNumber,
      address: e3ProgramAddress,
    },
    "MockE3Program",
    chain,
  );

  const mockProgramContract = MockE3ProgramFactory.connect(
    e3ProgramAddress,
    signer,
  );

  return { e3Program: mockProgramContract };
};
