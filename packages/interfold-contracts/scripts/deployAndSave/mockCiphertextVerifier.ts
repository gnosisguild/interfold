// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import type { HardhatRuntimeEnvironment } from "hardhat/types/hre";

import {
  MockCiphertextVerifier,
  MockCiphertextVerifier__factory as MockCiphertextVerifierFactory,
} from "../../types";
import { getDeploymentChain, storeDeploymentArgs } from "../utils";

export const deployAndSaveMockCiphertextVerifier = async (
  hre: HardhatRuntimeEnvironment,
): Promise<{ ciphertextVerifier: MockCiphertextVerifier }> => {
  const { ethers } = await hre.network.connect();
  const [signer] = await ethers.getSigners();
  const chain = getDeploymentChain(hre);
  const deployed = await ethers.deployContract("MockCiphertextVerifier");
  await deployed.waitForDeployment();
  const address = await deployed.getAddress();

  storeDeploymentArgs(
    { address, blockNumber: await ethers.provider.getBlockNumber() },
    "MockCiphertextVerifier",
    chain,
  );

  return {
    ciphertextVerifier: MockCiphertextVerifierFactory.connect(address, signer),
  };
};
