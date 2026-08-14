// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import type { HardhatRuntimeEnvironment } from "hardhat/types/hre";

import {
  BondedVotes,
  BondedVotes__factory as BondedVotesFactory,
} from "../../types";
import {
  getDeploymentChain,
  readDeploymentArgs,
  storeDeploymentArgs,
} from "../utils";

/**
 * The arguments for the deployAndSaveBondedVotes function
 */
export interface BondedVotesArgs {
  /** FOLD. Supplies wallet-held voting power and the total supply. */
  token: string;
  /** The bonded-history contract to read alongside the token. */
  checkpoints: string;
  hre: HardhatRuntimeEnvironment;
}

/**
 * Deploys the BondedVotes contract and saves the deployment arguments.
 *
 * This is the address governance points at, so it has to be resolvable from the deployment
 * inventory. The constructor reverts unless the token and the history agree on their ERC-6372
 * clock, which makes a mismatched pair fail here rather than silently answer for two unrelated
 * points in time.
 * @param param0 - The deployment arguments
 * @returns The deployed BondedVotes contract
 */
export const deployAndSaveBondedVotes = async ({
  token,
  checkpoints,
  hre,
}: BondedVotesArgs): Promise<{ bondedVotes: BondedVotes }> => {
  const { ethers } = await hre.network.connect();
  const [signer] = await ethers.getSigners();
  const chain = getDeploymentChain(hre);

  const preDeployedArgs = readDeploymentArgs("BondedVotes", chain);
  // Both references are immutable, so a record for a different pair cannot be reused.
  if (
    preDeployedArgs?.address &&
    preDeployedArgs?.constructorArgs?.token === token &&
    preDeployedArgs?.constructorArgs?.checkpoints === checkpoints
  ) {
    return {
      bondedVotes: BondedVotesFactory.connect(preDeployedArgs.address, signer),
    };
  }

  const factory = await ethers.getContractFactory("BondedVotes");
  const bondedVotes = await factory.deploy(token, checkpoints);
  await bondedVotes.waitForDeployment();

  const address = await bondedVotes.getAddress();
  storeDeploymentArgs(
    {
      constructorArgs: { token, checkpoints },
      blockNumber: await ethers.provider.getBlockNumber(),
      address,
    },
    "BondedVotes",
    chain,
  );

  return { bondedVotes: BondedVotesFactory.connect(address, signer) };
};
