// SPDX-License-Identifier: LGPL-3.0-only
import { connect } from "./cli";
import { deploymentPath, readJson, writeJson } from "./files";
import type { ProtocolDeployment } from "./types";
import { deployedAddress, loadConfig, requireContract } from "./values";

/**
 * Deploy `BondedVotes` and hand back the address governance points at.
 *
 * Split from `--action deploy` because it cannot run there. The constructor asks the registry which
 * token it bonds and refuses to build unless that matches the token it will read votes from, and
 * the registry is only initialized by the governance batch that `--action deploy` writes. Running this
 * before that batch executes fails loudly rather than producing an adapter bound to nothing.
 *
 * Nothing here needs a Safe transaction: `BondedVotes` holds no state and no privileges.
 */
export async function actionActivateVoting(): Promise<void> {
  const { ethers } = await connect();
  const config = loadConfig();
  const deployment = readJson<ProtocolDeployment>(deploymentPath(config));

  const network = await ethers.provider.getNetwork();
  if (Number(network.chainId) !== deployment.chainId) {
    throw new Error("Connected to the wrong network for this deployment file");
  }

  await requireContract(
    ethers.provider,
    deployment.bondedCheckpoints,
    "bondedCheckpoints",
  );

  const registry = await ethers.getContractAt(
    "BondingRegistry",
    deployment.bondingRegistryProxy,
  );

  // Fail here, with a readable message, rather than inside the constructor.
  const ciphernodeBondToken: string = await registry.getCiphernodeBondToken();
  if (ciphernodeBondToken.toLowerCase() !== config.fold.toLowerCase()) {
    throw new Error(
      `BondingRegistry bonds ${ciphernodeBondToken}, not FOLD (${config.fold}). ` +
        "Execute the governance batch from --action deploy first.",
    );
  }

  const attached: string = await registry.bondedCheckpoints();
  if (attached.toLowerCase() !== deployment.bondedCheckpoints.toLowerCase()) {
    throw new Error(
      `BondingRegistry has ${attached} attached, not the deployed ` +
        `${deployment.bondedCheckpoints}. Execute the governance batch first.`,
    );
  }

  if (deployment.bondedVotes) {
    console.log(`BondedVotes already deployed at ${deployment.bondedVotes}`);
    return;
  }

  const factory = await ethers.getContractFactory("BondedVotes");
  const bondedVotes = await factory.deploy(
    config.fold,
    deployment.bondedCheckpoints,
  );
  await bondedVotes.waitForDeployment();
  deployment.bondedVotes = await deployedAddress(bondedVotes);
  writeJson(deploymentPath(config), deployment);

  console.log(`
Bonded voting activated
  BondedCheckpoints: ${deployment.bondedCheckpoints}
  BondedVotes:       ${deployment.bondedVotes}

Point the governance plugin at BondedVotes as its voting token.
`);
}
