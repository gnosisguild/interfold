// SPDX-License-Identifier: LGPL-3.0-only
import type { HardhatRuntimeEnvironment } from "hardhat/types/hre";

import {
  NodeReleaseRegistry,
  NodeReleaseRegistry__factory as NodeReleaseRegistryFactory,
} from "../../types";
import {
  getDeploymentChain,
  readDeploymentArgs,
  storeDeploymentArgs,
} from "../utils";

export async function deployAndSaveNodeReleaseRegistry({
  owner,
  bondingRegistry,
  ciphernodeRegistry,
  hre,
}: {
  owner: string;
  bondingRegistry: string;
  ciphernodeRegistry: string;
  hre: HardhatRuntimeEnvironment;
}): Promise<{ nodeReleaseRegistry: NodeReleaseRegistry }> {
  const { ethers } = await hre.network.connect();
  const [signer] = await ethers.getSigners();
  const chain = getDeploymentChain(hre);
  const existing = readDeploymentArgs("NodeReleaseRegistry", chain);
  const args = existing?.constructorArgs as
    | {
        owner?: string;
        bondingRegistry?: string;
        ciphernodeRegistry?: string;
      }
    | undefined;
  if (
    existing?.address &&
    args?.owner?.toLowerCase() === owner.toLowerCase() &&
    args?.bondingRegistry?.toLowerCase() === bondingRegistry.toLowerCase() &&
    args?.ciphernodeRegistry?.toLowerCase() ===
      ciphernodeRegistry.toLowerCase() &&
    (await ethers.provider.getCode(existing.address)) !== "0x"
  ) {
    return {
      nodeReleaseRegistry: NodeReleaseRegistryFactory.connect(
        existing.address,
        signer,
      ),
    };
  }

  const nodeReleaseRegistry = await new NodeReleaseRegistryFactory(
    signer,
  ).deploy(owner, bondingRegistry, ciphernodeRegistry);
  await nodeReleaseRegistry.waitForDeployment();
  const address = await nodeReleaseRegistry.getAddress();
  const blockNumber = await ethers.provider.getBlockNumber();
  storeDeploymentArgs(
    {
      address,
      blockNumber,
      constructorArgs: { owner, bondingRegistry, ciphernodeRegistry },
    },
    "NodeReleaseRegistry",
    chain,
  );
  return { nodeReleaseRegistry };
}
