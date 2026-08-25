// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import type { HardhatRuntimeEnvironment } from "hardhat/types/hre";

import {
  MockRandomnessProvider,
  MockRandomnessProvider__factory as MockRandomnessProviderFactory,
} from "../../types";
import {
  getDeploymentChain,
  readDeploymentArgs,
  storeDeploymentArgs,
} from "../utils";

export async function deployAndSaveMockRandomnessProvider({
  requester,
  hre,
}: {
  requester: string;
  hre: HardhatRuntimeEnvironment;
}): Promise<{ randomnessProvider: MockRandomnessProvider }> {
  const { ethers } = await hre.network.connect();
  const [signer] = await ethers.getSigners();
  const chain = getDeploymentChain(hre);
  const existing = readDeploymentArgs("MockRandomnessProvider", chain);

  if (
    existing?.address &&
    String(existing.constructorArgs?.requester).toLowerCase() ===
      requester.toLowerCase()
  ) {
    return {
      randomnessProvider: MockRandomnessProviderFactory.connect(
        existing.address,
        signer,
      ),
    };
  }

  const randomnessProvider = await new MockRandomnessProviderFactory(
    signer,
  ).deploy(requester);
  await randomnessProvider.waitForDeployment();
  const address = await randomnessProvider.getAddress();
  const blockNumber = await ethers.provider.getBlockNumber();

  storeDeploymentArgs(
    {
      constructorArgs: { requester },
      address,
      blockNumber,
    },
    "MockRandomnessProvider",
    chain,
  );

  return { randomnessProvider };
}
