// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import type { HardhatRuntimeEnvironment } from "hardhat/types/hre";

import {
  BondedCheckpoints,
  BondedCheckpoints__factory as BondedCheckpointsFactory,
} from "../../types";
import {
  getDeploymentChain,
  readDeploymentArgs,
  storeDeploymentArgs,
} from "../utils";

/**
 * The arguments for the deployAndSaveBondedCheckpoints function
 */
export interface BondedCheckpointsArgs {
  /**
   * The BondingRegistry allowed to write history. Must be the proxy: that is the address that
   * calls `sync`, and the checkpoint contract accepts writes from exactly one address.
   */
  registry: string;
  hre: HardhatRuntimeEnvironment;
}

/**
 * Deploys the BondedCheckpoints contract and saves the deployment arguments.
 *
 * Recorded in the deployment inventory because the governance adapter is resolved from it: without
 * a record, a consumer cannot find the history that backs bonded voting power.
 * @param param0 - The deployment arguments
 * @returns The deployed BondedCheckpoints contract
 */
export const deployAndSaveBondedCheckpoints = async ({
  registry,
  hre,
}: BondedCheckpointsArgs): Promise<{
  bondedCheckpoints: BondedCheckpoints;
}> => {
  const { ethers } = await hre.network.connect();
  const [signer] = await ethers.getSigners();
  const chain = getDeploymentChain(hre);

  const preDeployedArgs = readDeploymentArgs("BondedCheckpoints", chain);
  // The registry is immutable here, so a record for another registry cannot be reused.
  if (
    preDeployedArgs?.address &&
    preDeployedArgs?.constructorArgs?.registry === registry
  ) {
    return {
      bondedCheckpoints: BondedCheckpointsFactory.connect(
        preDeployedArgs.address,
        signer,
      ),
    };
  }

  const factory = await ethers.getContractFactory("BondedCheckpoints");
  const bondedCheckpoints = await factory.deploy(registry);
  await bondedCheckpoints.waitForDeployment();

  const address = await bondedCheckpoints.getAddress();
  storeDeploymentArgs(
    {
      constructorArgs: { registry },
      blockNumber: await ethers.provider.getBlockNumber(),
      address,
    },
    "BondedCheckpoints",
    chain,
  );

  return {
    bondedCheckpoints: BondedCheckpointsFactory.connect(address, signer),
  };
};
