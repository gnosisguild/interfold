// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";
import fs from "fs";

import { syncProtocolDeploymentRecords } from "../deploymentRecords";
import { connect, hasFlag, networkName } from "./cli";
import { proxyAdminInterface } from "./constants";
import { deployProtocolContracts } from "./deployContracts";
import { deploymentPath, readJson, safeBatchPath, writeJson } from "./files";
import { governanceBatch, proposeSafeBatch } from "./safe";
import { buildSafeTransactions } from "./transactions";
import type { ProtocolDeployment, SafeTransaction } from "./types";
import { address, loadConfig, requireContract } from "./values";

async function assertPreconditions(
  ethers: any,
  config: ReturnType<typeof loadConfig>,
) {
  await Promise.all([
    requireContract(ethers.provider, config.fold, "fold"),
    requireContract(ethers.provider, config.feeToken, "feeToken"),
    requireContract(
      ethers.provider,
      config.ticketUnderlyingToken,
      "ticketUnderlyingToken",
    ),
    requireContract(
      ethers.provider,
      config.bondingRegistryProxy,
      "bondingRegistryProxy",
    ),
    requireContract(
      ethers.provider,
      config.bondingRegistryProxyAdmin,
      "bondingRegistryProxyAdmin",
    ),
    requireContract(ethers.provider, config.e3Programs[0], "e3Programs[0]"),
  ]);

  if (config.safe) {
    await requireContract(ethers.provider, config.safe, "safe");
  }
  if (config.ciphertextVerifier) {
    await requireContract(
      ethers.provider,
      config.ciphertextVerifier,
      "ciphertextVerifier",
    );
  }
  if (!config.verifiers?.deploy) {
    for (const [label, target] of [
      ["decryptionVerifier", config.verifiers?.decryptionVerifier],
      ["pkVerifier", config.verifiers?.pkVerifier],
      [
        "dkgFoldAttestationVerifier",
        config.verifiers?.dkgFoldAttestationVerifier,
      ],
    ] as const) {
      if (target) await requireContract(ethers.provider, target, label);
    }
  }

  const proxyAdmin = new ethersLib.Contract(
    config.bondingRegistryProxyAdmin,
    proxyAdminInterface,
    ethers.provider,
  );
  const proxyAdminOwner = address(await proxyAdmin.owner(), "proxyAdmin.owner");
  if (proxyAdminOwner !== config.protocolOwner) {
    throw new Error(
      `BondingRegistry ProxyAdmin owner mismatch: expected ${config.protocolOwner}, got ${proxyAdminOwner}`,
    );
  }
}

export async function actionDeploy(): Promise<void> {
  const { ethers } = await connect();
  const network = await ethers.provider.getNetwork();
  const chainId = Number(network.chainId);
  const config = loadConfig();
  if (chainId !== config.chainId) {
    throw new Error(
      `Connected chainId ${chainId} != config.chainId ${config.chainId}`,
    );
  }

  const [operator] = await ethers.getSigners();
  const operatorAddress = await operator.getAddress();
  await assertPreconditions(ethers, config);

  console.log(`Deploying protocol contracts for ${config.name}`);

  const result = await deployProtocolContracts(ethers, operator, config);
  const blockNumber = await ethers.provider.getBlockNumber();
  const txs = buildSafeTransactions(
    config,
    result.contracts,
    result.interfaces,
  );
  const batchFile = safeBatchPath(config);
  writeJson(batchFile, governanceBatch(config, txs));

  const deployment: ProtocolDeployment = {
    name: config.name,
    chainId,
    operator: operatorAddress,
    protocolOwner: config.protocolOwner,
    safe: config.safe,
    fold: config.fold,
    feeToken: config.feeToken,
    ticketUnderlyingToken: config.ticketUnderlyingToken,
    bondingRegistryProxy: config.bondingRegistryProxy,
    bondingRegistryProxyAdmin: config.bondingRegistryProxyAdmin,
    ...result.contracts,
    safeTransactions: batchFile,
  };
  const deploymentFile = deploymentPath(config);
  writeJson(deploymentFile, deployment);
  syncProtocolDeploymentRecords(config, deployment, result.interfaces, {
    chain: networkName(),
    blockNumber,
    syncIntegrationConfig: hasFlag("sync-integration-config"),
  });

  if (hasFlag("propose-safe")) {
    deployment.safeProposal = await proposeSafeBatch(config, txs);
    writeJson(deploymentFile, deployment);
    printProposal(deployment.safeProposal);
  }

  console.log(`
Protocol contracts deployed
  fee token:              ${deployment.feeToken}
  ticket underlying:      ${deployment.ticketUnderlyingToken}
  ticketToken:            ${deployment.ticketToken}
  slashingManager:        ${deployment.slashingManager}
  slashingEvidenceLib:    ${deployment.slashingEvidenceLib}
  ciphernodeRegistry:     ${deployment.ciphernodeRegistry}
  interfold:              ${deployment.interfold}
  interfoldLifecycle:     ${deployment.interfoldLifecycle}
  interfoldPricing:       ${deployment.interfoldPricing}
  e3RefundManager:        ${deployment.e3RefundManager}
  bondingAssetLib:        ${deployment.bondingAssetLib}
  bondingEligibilityLib:  ${deployment.bondingEligibilityLib}
  bondingSlashingLib:     ${deployment.bondingSlashingLib}
  bonding implementation: ${deployment.bondingRegistryImplementation}
  bondedCheckpoints:      ${deployment.bondedCheckpoints}
  bondedVotes:            (run --action activate-voting after the governance batch)

Governance batch required
  file: ${batchFile}
  txs:  ${txs.length}

Deployment file
  ${deploymentFile}
`);
}

export async function actionProposeSafe(): Promise<void> {
  const config = loadConfig();
  const transactions = readGovernanceBatch(config);
  const proposal = await proposeSafeBatch(config, transactions);

  if (fs.existsSync(deploymentPath(config))) {
    const deployment = readJson<ProtocolDeployment>(deploymentPath(config));
    deployment.safeProposal = proposal;
    writeJson(deploymentPath(config), deployment);
  }

  printProposal(proposal);
}

export async function actionExecuteGovernance(): Promise<void> {
  const { ethers } = await connect();
  const config = loadConfig();
  const network = await ethers.provider.getNetwork();
  const chainId = Number(network.chainId);
  if (chainId !== config.chainId) {
    throw new Error(
      `Connected chainId ${chainId} != config.chainId ${config.chainId}`,
    );
  }
  if (chainId === 1) {
    throw new Error(
      "Direct governance execution is disabled on mainnet. Submit the transaction file through the DAO proposal flow.",
    );
  }

  const [signer] = await ethers.getSigners();
  const signerAddress = address(await signer.getAddress(), "signer");
  if (signerAddress !== config.protocolOwner) {
    throw new Error(
      `Protocol owner mismatch: signer is ${signerAddress}, expected ${config.protocolOwner}`,
    );
  }

  const transactions = readGovernanceBatch(config);
  for (let index = 0; index < transactions.length; index++) {
    const tx = transactions[index];
    if (tx.operation !== 0) {
      throw new Error(`Transaction ${index + 1} is not a CALL operation`);
    }
    const response = await signer.sendTransaction({
      to: tx.to,
      value: BigInt(tx.value),
      data: tx.data,
    });
    await response.wait();
    console.log(
      `  executed ${index + 1}/${transactions.length}: ${response.hash}`,
    );
  }
}

function readGovernanceBatch(
  config: ReturnType<typeof loadConfig>,
): SafeTransaction[] {
  const file = safeBatchPath(config);
  if (!fs.existsSync(file)) {
    throw new Error(
      `Governance batch not found: ${file}. Run --action deploy first.`,
    );
  }
  const batch = readJson<{ transactions?: SafeTransaction[] }>(file);
  if (!Array.isArray(batch.transactions)) {
    throw new Error(`Governance batch has no transactions array: ${file}`);
  }
  return batch.transactions;
}

function printProposal(
  proposal: NonNullable<ProtocolDeployment["safeProposal"]>,
) {
  console.log(`
Safe transaction proposed
  hash: ${proposal.safeTxHash}
  nonce: ${proposal.nonce}
  txs:  ${proposal.transactionCount}
  url:  ${proposal.url ?? "(open the Safe UI pending queue)"}
`);
}
