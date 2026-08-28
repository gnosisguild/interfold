// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { arg, connect, hasFlag, networkName } from "../protocol/cli";
import { BFV_PARAMS, ZERO, proxyAdminInterface } from "../protocol/constants";
import { deployBfvVerifierRoutes } from "../protocol/deployContracts";
import {
  deploymentPath,
  governanceSafeBuilderPath,
  protocolDir,
  readJson,
  repoRoot,
  resolvePath,
  writeJson,
} from "../protocol/files";
import {
  currentNodeRelease,
  requiredCircuitsVersion,
} from "../protocol/nodeRelease";
import {
  aragonAdminSafeBatch,
  aragonAdminSafeTransactions,
  governanceBatch,
  proposeSafeBatch,
  safeTx,
} from "../protocol/safe";
import type {
  ProtocolConfigFile,
  ProtocolDeployment,
  SafeTransaction,
  SecureCrispUpgradePlan,
} from "../protocol/types";
import {
  address,
  encodeBfvParams,
  loadConfig,
  requireContract,
} from "../protocol/values";
import { MAINNET_BFV_CONFIGS, PRODUCTION_BFV_CONFIG } from "../utils";
import {
  deployUpgradeImplementation,
  proxyImplementation,
} from "./safeProxyUpgrade";
import { expectedCrispImageId } from "./secureCrispArtifacts";

const BFV_SCHEME_ID = ethersLib.id("fhe.rs:BFV");
const SECURE_PARAM_SET = 1;
const crispInterface = new ethersLib.Interface([
  "function bindInterfold(address interfold)",
  "function interfold() view returns (address)",
  "function owner() view returns (address)",
  "function imageId() view returns (bytes32)",
  "function risc0Verifier() view returns (address)",
]);
const ciphertextInterface = new ethersLib.Interface([
  "function imageId() view returns (bytes32)",
  "function risc0Verifier() view returns (address)",
]);

type CrispDeploymentRecord = Record<
  string,
  Record<string, { address?: string }>
>;

function planPath(config: ProtocolConfigFile): string {
  return path.join(protocolDir, `${config.name}.secure-crisp.upgrade.json`);
}

function batchPath(config: ProtocolConfigFile): string {
  return path.join(
    protocolDir,
    `${config.name}.secure-crisp.upgrade.safe.json`,
  );
}

function defaultCrispDeploymentsPath(): string {
  return path.join(
    repoRoot,
    "examples",
    "CRISP",
    "packages",
    "crisp-contracts",
    "deployed_contracts.json",
  );
}

export function resolveCrispAddresses(): {
  crispProgram: string;
  ciphertextVerifier: string;
} {
  const crispProgramOverride = arg("crisp-program");
  const ciphertextOverride = arg("ciphertext-verifier");
  if (Boolean(crispProgramOverride) !== Boolean(ciphertextOverride)) {
    throw new Error(
      "Pass both --crisp-program and --ciphertext-verifier, or neither",
    );
  }
  if (crispProgramOverride && ciphertextOverride) {
    return {
      crispProgram: address(crispProgramOverride, "CRISP program"),
      ciphertextVerifier: address(
        ciphertextOverride,
        "CRISP ciphertext verifier",
      ),
    };
  }

  const file = resolvePath(
    arg("crisp-deployments") ?? defaultCrispDeploymentsPath(),
  );
  if (!fs.existsSync(file)) {
    throw new Error(`CRISP deployment file not found: ${file}`);
  }
  const deployments = readJson<CrispDeploymentRecord>(file);
  const deployment = deployments[networkName()];
  if (!deployment) {
    throw new Error(`No CRISP deployment is recorded for ${networkName()}`);
  }
  return {
    crispProgram: address(
      deployment.CRISPProgram?.address ?? "",
      "CRISPProgram",
    ),
    ciphertextVerifier: address(
      deployment.Risc0BfvCiphertextVerifier?.address ?? "",
      "Risc0BfvCiphertextVerifier",
    ),
  };
}

export function upgradeTransaction(
  proxyAdmin: string,
  proxy: string,
  implementation: string,
): SafeTransaction {
  return safeTx(
    proxyAdmin,
    proxyAdminInterface.encodeFunctionData("upgradeAndCall", [
      proxy,
      implementation,
      "0x",
    ]),
  );
}

async function requireProxyAdminOwner(
  ethers: any,
  proxyAdmin: string,
  expectedOwner: string,
): Promise<void> {
  await requireContract(ethers.provider, proxyAdmin, "Interfold ProxyAdmin");
  const admin = await ethers.getContractAt("ProxyAdmin", proxyAdmin);
  const owner = await admin.owner();
  if (owner.toLowerCase() !== expectedOwner.toLowerCase()) {
    throw new Error(
      `Interfold ProxyAdmin owner mismatch: expected ${expectedOwner}, got ${owner}`,
    );
  }
}

async function readContract(
  provider: any,
  target: string,
  contractInterface: ethersLib.Interface,
  functionName: string,
): Promise<any> {
  const data = contractInterface.encodeFunctionData(functionName);
  const result = await provider.call({ to: target, data });
  return contractInterface.decodeFunctionResult(functionName, result)[0];
}

export async function prepareSecureCrispUpgrade(): Promise<void> {
  const { ethers } = await connect();
  const config = loadConfig();
  const deployment = readJson<ProtocolDeployment>(deploymentPath(config));
  const network = await ethers.provider.getNetwork();
  if (Number(network.chainId) !== 1 || config.chainId !== 1) {
    throw new Error("The secure CRISP activation script is Ethereum-only");
  }
  if (deployment.chainId !== 1) {
    throw new Error("The protocol deployment file is not for Ethereum mainnet");
  }
  if (!config.governance) {
    throw new Error("Aragon governance is required for mainnet activation");
  }

  const { crispProgram, ciphertextVerifier } = resolveCrispAddresses();
  await Promise.all([
    requireContract(ethers.provider, deployment.interfold, "Interfold proxy"),
    requireContract(
      ethers.provider,
      deployment.ciphernodeRegistry,
      "CiphernodeRegistry proxy",
    ),
    requireContract(
      ethers.provider,
      deployment.nodeReleaseRegistry,
      "NodeReleaseRegistry",
    ),
    requireContract(ethers.provider, crispProgram, "CRISP program"),
    requireContract(
      ethers.provider,
      ciphertextVerifier,
      "CRISP ciphertext verifier",
    ),
    requireProxyAdminOwner(
      ethers,
      deployment.interfoldProxyAdmin,
      config.protocolOwner,
    ),
  ]);

  const interfold = await ethers.getContractAt(
    "Interfold",
    deployment.interfold,
  );
  const registry = await ethers.getContractAt(
    "CiphernodeRegistryOwnable",
    deployment.ciphernodeRegistry,
  );
  const releases = await ethers.getContractAt(
    "NodeReleaseRegistry",
    deployment.nodeReleaseRegistry,
  );
  if (!(await interfold.requestsPaused())) {
    throw new Error(
      "Pause E3 requests before preparing secure CRISP activation",
    );
  }
  if ((await interfold.activeE3Count()) !== 0n) {
    throw new Error("Interfold still has active E3s");
  }
  if ((await registry.unreleasedCommitteeCount()) !== 0n) {
    throw new Error("CiphernodeRegistry still has unreleased committees");
  }
  const interfoldOwner = await interfold.owner();
  if (interfoldOwner.toLowerCase() !== config.protocolOwner.toLowerCase()) {
    throw new Error(
      `Interfold owner mismatch: expected ${config.protocolOwner}, got ${interfoldOwner}`,
    );
  }
  const releaseBindings = [
    [
      "Interfold node release registry",
      await interfold.nodeReleaseRegistry(),
      deployment.nodeReleaseRegistry,
    ],
    ["NodeReleaseRegistry owner", await releases.owner(), config.protocolOwner],
    [
      "NodeReleaseRegistry BondingRegistry",
      await releases.bondingRegistry(),
      config.bondingRegistryProxy,
    ],
    [
      "NodeReleaseRegistry CiphernodeRegistry",
      await releases.ciphernodeRegistry(),
      deployment.ciphernodeRegistry,
    ],
  ] as const;
  for (const [label, actual, expected] of releaseBindings) {
    if (String(actual).toLowerCase() !== expected.toLowerCase()) {
      throw new Error(`${label} mismatch: expected ${expected}, got ${actual}`);
    }
  }
  const nodeRelease = currentNodeRelease();
  const circuitsVersion = requiredCircuitsVersion();
  if (nodeRelease.version !== circuitsVersion) {
    throw new Error(
      `Secure CRISP requires a release whose binary and circuit archive match; source version is ${nodeRelease.version}, circuit archive is ${circuitsVersion}`,
    );
  }
  const [requiredProtocolVersion, requiredNodeGeneration] = await Promise.all([
    releases.requiredProtocolVersion(),
    releases.requiredNodeGeneration(),
  ]);
  if (BigInt(nodeRelease.protocolVersion) <= requiredProtocolVersion) {
    throw new Error(
      `Secure CRISP requires a new protocol_version above ${requiredProtocolVersion}; this release declares ${nodeRelease.protocolVersion}`,
    );
  }
  if (BigInt(nodeRelease.nodeGeneration) < requiredNodeGeneration) {
    throw new Error(
      `node_generation cannot move backwards from ${requiredNodeGeneration} to ${nodeRelease.nodeGeneration}`,
    );
  }
  const liveImplementation = await proxyImplementation(
    ethers,
    deployment.interfold,
  );
  if (
    liveImplementation.toLowerCase() !==
    deployment.interfoldImplementation.toLowerCase()
  ) {
    throw new Error(
      `Interfold deployment record is stale: recorded ${deployment.interfoldImplementation}, live ${liveImplementation}`,
    );
  }

  const crispOwner = await readContract(
    ethers.provider,
    crispProgram,
    crispInterface,
    "owner",
  );
  if (String(crispOwner).toLowerCase() !== config.protocolOwner.toLowerCase()) {
    throw new Error(
      `CRISP owner mismatch: expected ${config.protocolOwner}, got ${crispOwner}`,
    );
  }
  const boundInterfold = await readContract(
    ethers.provider,
    crispProgram,
    crispInterface,
    "interfold",
  );
  const normalizedBoundInterfold = String(boundInterfold).toLowerCase();
  if (
    normalizedBoundInterfold !== ZERO.toLowerCase() &&
    normalizedBoundInterfold !== deployment.interfold.toLowerCase()
  ) {
    throw new Error(`CRISP is already bound to ${boundInterfold}`);
  }

  const [crispImageId, verifierImageId, crispRisc0, verifierRisc0] =
    await Promise.all([
      readContract(ethers.provider, crispProgram, crispInterface, "imageId"),
      readContract(
        ethers.provider,
        ciphertextVerifier,
        ciphertextInterface,
        "imageId",
      ),
      readContract(
        ethers.provider,
        crispProgram,
        crispInterface,
        "risc0Verifier",
      ),
      readContract(
        ethers.provider,
        ciphertextVerifier,
        ciphertextInterface,
        "risc0Verifier",
      ),
    ]);
  if (crispImageId !== verifierImageId) {
    throw new Error(
      "CRISP and its ciphertext verifier use different image IDs",
    );
  }
  const expectedImageId = expectedCrispImageId();
  if (String(crispImageId).toLowerCase() !== expectedImageId) {
    throw new Error(
      `CRISP image ID mismatch: expected ${expectedImageId}, got ${crispImageId}`,
    );
  }
  if (
    String(crispRisc0).toLowerCase() !== String(verifierRisc0).toLowerCase()
  ) {
    throw new Error(
      "CRISP and its ciphertext verifier use different RISC Zero verifiers",
    );
  }
  await requireContract(
    ethers.provider,
    String(crispRisc0),
    "CRISP RISC Zero verifier",
  );

  const secureParams = encodeBfvParams(BFV_PARAMS.secure8192);
  const currentParams = await interfold.paramSetRegistry(SECURE_PARAM_SET);
  if (
    currentParams !== "0x" &&
    currentParams.toLowerCase() !== secureParams.toLowerCase()
  ) {
    throw new Error(
      "The registered secure BFV parameter set does not match this release",
    );
  }

  const [operator] = await ethers.getSigners();
  const interfoldUpgrade = await deployUpgradeImplementation(
    ethers,
    operator,
    "interfold",
    deployment,
  );
  if (!interfoldUpgrade.lifecycleLibrary || !interfoldUpgrade.pricingLibrary) {
    throw new Error("Interfold libraries were not deployed");
  }
  const verifierDeployment = await deployBfvVerifierRoutes(
    ethers,
    deployment.ciphernodeRegistry,
    PRODUCTION_BFV_CONFIG,
    MAINNET_BFV_CONFIGS,
  );

  const txs: SafeTransaction[] = [
    upgradeTransaction(
      deployment.interfoldProxyAdmin,
      deployment.interfold,
      interfoldUpgrade.implementation,
    ),
  ];
  if (currentParams === "0x") {
    txs.push(
      safeTx(
        deployment.interfold,
        interfold.interface.encodeFunctionData("setParamSet", [
          SECURE_PARAM_SET,
          secureParams,
        ]),
      ),
    );
  }
  for (const threshold of config.interfold.committeeThresholds) {
    const current = await interfold.committeeThresholds(BigInt(threshold.size));
    if (
      current[0] !== BigInt(threshold.quorum) ||
      current[1] !== BigInt(threshold.total)
    ) {
      txs.push(
        safeTx(
          deployment.interfold,
          interfold.interface.encodeFunctionData("setCommitteeThresholds", [
            BigInt(threshold.size),
            [BigInt(threshold.quorum), BigInt(threshold.total)],
          ]),
        ),
      );
    }
  }
  txs.push(
    safeTx(
      deployment.interfold,
      interfold.interface.encodeFunctionData("setPkVerifier", [
        BFV_SCHEME_ID,
        verifierDeployment.pkVerifier,
      ]),
    ),
    safeTx(
      deployment.interfold,
      interfold.interface.encodeFunctionData("setDecryptionVerifier", [
        BFV_SCHEME_ID,
        verifierDeployment.decryptionVerifier,
      ]),
    ),
  );
  if (
    String(
      await interfold.getCiphertextVerifier(BFV_SCHEME_ID),
    ).toLowerCase() !== ciphertextVerifier.toLowerCase()
  ) {
    txs.push(
      safeTx(
        deployment.interfold,
        interfold.interface.encodeFunctionData("setCiphertextVerifier", [
          BFV_SCHEME_ID,
          ciphertextVerifier,
        ]),
      ),
    );
  }
  if (!(await interfold.e3Programs(crispProgram))) {
    txs.push(
      safeTx(
        deployment.interfold,
        interfold.interface.encodeFunctionData("registerE3Program", [
          crispProgram,
        ]),
      ),
    );
  }
  if (normalizedBoundInterfold === ZERO.toLowerCase()) {
    txs.push(
      safeTx(
        crispProgram,
        crispInterface.encodeFunctionData("bindInterfold", [
          deployment.interfold,
        ]),
      ),
    );
  }
  txs.push(
    safeTx(
      deployment.nodeReleaseRegistry,
      releases.interface.encodeFunctionData("setRequiredNodeRelease", [
        nodeRelease.protocolVersion,
        nodeRelease.nodeGeneration,
      ]),
    ),
  );

  const rawBatchFile = batchPath(config);
  const batch = governanceBatch(config, txs);
  batch.meta.name = `${config.name} secure CRISP activation`;
  batch.meta.description =
    "Install secure BFV routes, bind CRISP, and require the matching ciphernode protocol while requests remain paused.";
  writeJson(rawBatchFile, batch);

  const safeBuilderFile = governanceSafeBuilderPath({
    ...config,
    name: `${config.name}.secure-crisp.upgrade`,
  });
  const safeBatch = aragonAdminSafeBatch(config, txs);
  safeBatch.meta.name = `${config.name} secure CRISP activation`;
  writeJson(safeBuilderFile, safeBatch);

  const plan: SecureCrispUpgradePlan = {
    name: config.name,
    chainId: config.chainId,
    operator: await operator.getAddress(),
    protocolOwner: config.protocolOwner,
    interfoldProxy: deployment.interfold,
    interfoldProxyAdmin: deployment.interfoldProxyAdmin,
    interfoldImplementation: interfoldUpgrade.implementation,
    lifecycleLibrary: interfoldUpgrade.lifecycleLibrary,
    pricingLibrary: interfoldUpgrade.pricingLibrary,
    registryProxy: deployment.ciphernodeRegistry,
    nodeReleaseRegistry: deployment.nodeReleaseRegistry,
    nodeRelease,
    cryptoConfigId: PRODUCTION_BFV_CONFIG.configId,
    paramSet: SECURE_PARAM_SET,
    pkVerifier: verifierDeployment.pkVerifier,
    decryptionVerifier: verifierDeployment.decryptionVerifier,
    ciphertextVerifier,
    crispProgram,
    bfvVerifierRoutes: verifierDeployment.bfvVerifierRoutes,
    safeTransactions: rawBatchFile,
    governanceSafeBuilder: safeBuilderFile,
  };
  if (hasFlag("propose-safe")) {
    plan.safeProposal = await proposeSafeBatch(
      config,
      aragonAdminSafeTransactions(config, txs),
      config.governance.proposerSafe,
    );
  }
  writeJson(planPath(config), plan);

  console.log(`
Secure CRISP activation prepared
  Interfold implementation: ${plan.interfoldImplementation}
  PK verifier router:        ${plan.pkVerifier}
  decryption router:         ${plan.decryptionVerifier}
  CRISP program:             ${plan.crispProgram}
  required node release:     ${plan.nodeRelease.version} (protocol ${plan.nodeRelease.protocolVersion}, generation ${plan.nodeRelease.nodeGeneration})
  governance batch:          ${plan.safeTransactions}
  Aragon Safe batch:         ${plan.governanceSafeBuilder}
  transactions:              ${txs.length}
  requests remain paused after execution
`);
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  prepareSecureCrispUpgrade().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
