// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { connect } from "../protocol/cli";
import { BFV_PARAMS } from "../protocol/constants";
import {
  deploymentPath,
  protocolDir,
  readJson,
  writeJson,
} from "../protocol/files";
import {
  currentNodeRelease,
  requiredCircuitsVersion,
} from "../protocol/nodeRelease";
import type {
  ProtocolConfigFile,
  ProtocolDeployment,
  SecureCrispUpgradePlan,
} from "../protocol/types";
import {
  encodeBfvParams,
  loadConfig,
  requireContract,
} from "../protocol/values";
import {
  MAINNET_BFV_CONFIGS,
  PRODUCTION_BFV_CONFIG,
  getBfvDecryptionSubCircuitVkHashPaths,
  getBfvPkSubCircuitVkHashPaths,
  readVkRecursiveHash,
} from "../utils";
import { proxyImplementation } from "./safeProxyUpgrade";
import { expectedCrispImageId } from "./secureCrispArtifacts";

const BFV_SCHEME_ID = ethersLib.id("fhe.rs:BFV");
const crispInterface = new ethersLib.Interface([
  "function interfold() view returns (address)",
  "function imageId() view returns (bytes32)",
  "function risc0Verifier() view returns (address)",
]);
const ciphertextInterface = new ethersLib.Interface([
  "function imageId() view returns (bytes32)",
  "function risc0Verifier() view returns (address)",
]);

function planPath(config: ProtocolConfigFile): string {
  return path.join(protocolDir, `${config.name}.secure-crisp.upgrade.json`);
}

function equalAddress(actual: string, expected: string, label: string): void {
  if (actual.toLowerCase() !== expected.toLowerCase()) {
    throw new Error(`${label} mismatch: expected ${expected}, got ${actual}`);
  }
}

function equalValue(actual: unknown, expected: unknown, label: string): void {
  if (String(actual).toLowerCase() !== String(expected).toLowerCase()) {
    throw new Error(`${label} mismatch: expected ${expected}, got ${actual}`);
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

export async function validateSecureCrispUpgrade(): Promise<void> {
  const { ethers } = await connect();
  const config = loadConfig();
  const deploymentFile = deploymentPath(config);
  const deployment = readJson<ProtocolDeployment>(deploymentFile);
  const plan = readJson<SecureCrispUpgradePlan>(planPath(config));
  const network = await ethers.provider.getNetwork();
  if (
    Number(network.chainId) !== 1 ||
    config.chainId !== 1 ||
    deployment.chainId !== 1 ||
    plan.chainId !== 1
  ) {
    throw new Error("Secure CRISP validation is Ethereum-only");
  }
  if (plan.name !== config.name) {
    throw new Error(
      `Upgrade plan name mismatch: expected ${config.name}, got ${plan.name}`,
    );
  }
  equalAddress(
    plan.protocolOwner,
    config.protocolOwner,
    "upgrade plan protocol owner",
  );
  equalValue(
    plan.cryptoConfigId,
    PRODUCTION_BFV_CONFIG.configId,
    "upgrade plan crypto config",
  );
  equalValue(plan.paramSet, 1, "upgrade plan BFV parameter set");
  equalAddress(plan.interfoldProxy, deployment.interfold, "Interfold proxy");
  equalAddress(
    plan.interfoldProxyAdmin,
    deployment.interfoldProxyAdmin,
    "Interfold ProxyAdmin",
  );
  equalAddress(
    plan.registryProxy,
    deployment.ciphernodeRegistry,
    "CiphernodeRegistry proxy",
  );
  equalAddress(
    plan.nodeReleaseRegistry,
    deployment.nodeReleaseRegistry,
    "NodeReleaseRegistry",
  );
  const sourceRelease = currentNodeRelease();
  equalValue(
    sourceRelease.version,
    requiredCircuitsVersion(),
    "release circuit archive version",
  );
  equalValue(
    plan.nodeRelease.version,
    sourceRelease.version,
    "node release version",
  );
  equalValue(
    plan.nodeRelease.protocolVersion,
    sourceRelease.protocolVersion,
    "node release protocol version",
  );
  equalValue(
    plan.nodeRelease.nodeGeneration,
    sourceRelease.nodeGeneration,
    "node release generation",
  );
  equalValue(
    plan.nodeRelease.releaseId,
    sourceRelease.releaseId,
    "node release ID",
  );

  const codeAddresses = [
    [plan.interfoldImplementation, "Interfold implementation"],
    [plan.lifecycleLibrary, "InterfoldLifecycle"],
    [plan.pricingLibrary, "InterfoldPricing"],
    [plan.pkVerifier, "BFV PK router"],
    [plan.decryptionVerifier, "BFV decryption router"],
    [plan.ciphertextVerifier, "CRISP ciphertext verifier"],
    [plan.crispProgram, "CRISP program"],
    [plan.nodeReleaseRegistry, "NodeReleaseRegistry"],
    ...plan.bfvVerifierRoutes.flatMap((route) => [
      [route.pkVerifier, `${route.preset}/${route.committee} PK verifier`],
      [
        route.decryptionVerifier,
        `${route.preset}/${route.committee} decryption verifier`,
      ],
      [
        route.dkgAggregatorVerifier,
        `${route.preset}/${route.committee} DKG aggregator verifier`,
      ],
      [
        route.decryptionAggregatorVerifier,
        `${route.preset}/${route.committee} decryption aggregator verifier`,
      ],
      [
        route.verifierZkTranscriptLib,
        `${route.preset}/${route.committee} transcript library`,
      ],
      [
        route.dkgVerifierRelationsLib,
        `${route.preset}/${route.committee} DKG relations library`,
      ],
      [
        route.decryptionVerifierRelationsLib,
        `${route.preset}/${route.committee} decryption relations library`,
      ],
    ]),
  ] as Array<[string, string]>;
  await Promise.all(
    codeAddresses.map(([target, label]) =>
      requireContract(ethers.provider, target, label),
    ),
  );
  equalAddress(
    await proxyImplementation(ethers, deployment.interfold),
    plan.interfoldImplementation,
    "live Interfold implementation",
  );

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
    plan.nodeReleaseRegistry,
  );
  if (!(await interfold.requestsPaused())) {
    throw new Error("E3 requests must remain paused during validation");
  }
  equalAddress(
    await interfold.owner(),
    config.protocolOwner,
    "Interfold owner",
  );
  equalValue(await interfold.activeE3Count(), 0n, "active E3 count");
  equalValue(
    await registry.unreleasedCommitteeCount(),
    0n,
    "unreleased committee count",
  );
  equalAddress(
    await interfold.nodeReleaseRegistry(),
    plan.nodeReleaseRegistry,
    "Interfold node release registry",
  );
  equalAddress(
    await releases.owner(),
    config.protocolOwner,
    "NodeReleaseRegistry owner",
  );
  equalAddress(
    await releases.bondingRegistry(),
    config.bondingRegistryProxy,
    "NodeReleaseRegistry BondingRegistry",
  );
  equalAddress(
    await releases.ciphernodeRegistry(),
    deployment.ciphernodeRegistry,
    "NodeReleaseRegistry CiphernodeRegistry",
  );
  equalValue(
    await releases.requiredProtocolVersion(),
    plan.nodeRelease.protocolVersion,
    "required protocol version",
  );
  equalValue(
    await releases.requiredNodeGeneration(),
    plan.nodeRelease.nodeGeneration,
    "required node generation",
  );
  equalValue(
    await interfold.activeCryptoConfigId(),
    PRODUCTION_BFV_CONFIG.configId,
    "active crypto config",
  );
  equalValue(
    await interfold.paramSetRegistry(1),
    encodeBfvParams(BFV_PARAMS.secure8192),
    "secure BFV parameter set",
  );
  for (const threshold of config.interfold.committeeThresholds) {
    const actual = await interfold.committeeThresholds(BigInt(threshold.size));
    equalValue(
      actual[0],
      BigInt(threshold.quorum),
      `committee ${threshold.size} quorum`,
    );
    equalValue(
      actual[1],
      BigInt(threshold.total),
      `committee ${threshold.size} total`,
    );
  }
  equalAddress(
    await interfold.pkVerifiers(BFV_SCHEME_ID),
    plan.pkVerifier,
    "BFV PK verifier",
  );
  equalAddress(
    await interfold.decryptionVerifiers(BFV_SCHEME_ID),
    plan.decryptionVerifier,
    "BFV decryption verifier",
  );
  equalAddress(
    await interfold.getCiphertextVerifier(BFV_SCHEME_ID),
    plan.ciphertextVerifier,
    "BFV ciphertext verifier",
  );
  if (!(await interfold.e3Programs(plan.crispProgram))) {
    throw new Error("CRISP program is not registered");
  }
  equalAddress(
    String(
      await readContract(
        ethers.provider,
        plan.crispProgram,
        crispInterface,
        "interfold",
      ),
    ),
    deployment.interfold,
    "CRISP Interfold binding",
  );

  const crispImage = await readContract(
    ethers.provider,
    plan.crispProgram,
    crispInterface,
    "imageId",
  );
  const ciphertextImage = await readContract(
    ethers.provider,
    plan.ciphertextVerifier,
    ciphertextInterface,
    "imageId",
  );
  equalValue(ciphertextImage, crispImage, "CRISP image ID");
  equalValue(crispImage, expectedCrispImageId(), "release CRISP image ID");
  const crispRisc0 = String(
    await readContract(
      ethers.provider,
      plan.crispProgram,
      crispInterface,
      "risc0Verifier",
    ),
  );
  const ciphertextRisc0 = String(
    await readContract(
      ethers.provider,
      plan.ciphertextVerifier,
      ciphertextInterface,
      "risc0Verifier",
    ),
  );
  equalAddress(ciphertextRisc0, crispRisc0, "CRISP RISC Zero verifier");
  await requireContract(
    ethers.provider,
    crispRisc0,
    "CRISP RISC Zero verifier",
  );

  if (plan.bfvVerifierRoutes.length !== MAINNET_BFV_CONFIGS.length) {
    throw new Error(
      `Expected ${MAINNET_BFV_CONFIGS.length} secure BFV routes, got ${plan.bfvVerifierRoutes.length}`,
    );
  }
  const pkRouter = await ethers.getContractAt(
    "BfvPkVerifierRouter",
    plan.pkVerifier,
  );
  const decryptionRouter = await ethers.getContractAt(
    "BfvDecryptionVerifierRouter",
    plan.decryptionVerifier,
  );
  equalValue(
    await pkRouter.h(),
    PRODUCTION_BFV_CONFIG.h,
    "PK router default h",
  );
  equalValue(
    await decryptionRouter.threshold(),
    PRODUCTION_BFV_CONFIG.t,
    "decryption router default threshold",
  );
  const expectedRouteCount = BigInt(MAINNET_BFV_CONFIGS.length);
  equalValue(await pkRouter.routeCount(), expectedRouteCount, "PK route count");
  equalValue(
    await decryptionRouter.routeCount(),
    expectedRouteCount,
    "decryption route count",
  );

  for (let index = 0; index < MAINNET_BFV_CONFIGS.length; index += 1) {
    const expected = MAINNET_BFV_CONFIGS[index];
    const recorded = plan.bfvVerifierRoutes[index];
    if (
      recorded.preset !== expected.preset ||
      recorded.committee !== expected.committee ||
      recorded.paramSet !== expected.paramSet ||
      recorded.committeeSize !== expected.committeeSize
    ) {
      throw new Error(
        `Recorded BFV route ${index} does not match the release matrix`,
      );
    }

    const pkRoute = await pkRouter.routeAt(index);
    const decryptionRoute = await decryptionRouter.routeAt(index);
    equalAddress(pkRoute[0], recorded.pkVerifier, `PK route ${index}`);
    equalValue(
      pkRoute[1],
      3 * expected.h + 6,
      `PK route ${index} public input count`,
    );
    equalAddress(
      decryptionRoute[0],
      recorded.decryptionVerifier,
      `decryption route ${index}`,
    );
    equalValue(
      decryptionRoute[1],
      111 + 3 * expected.t,
      `decryption route ${index} public input count`,
    );

    const pkVerifier = await ethers.getContractAt(
      "BfvPkVerifier",
      recorded.pkVerifier,
    );
    const decryptionVerifier = await ethers.getContractAt(
      "BfvDecryptionVerifier",
      recorded.decryptionVerifier,
    );
    equalValue(await pkVerifier.h(), expected.h, `PK route ${index} h`);
    equalValue(
      await decryptionVerifier.threshold(),
      expected.t,
      `decryption route ${index} threshold`,
    );
    equalAddress(
      await pkVerifier.circuitVerifier(),
      recorded.dkgAggregatorVerifier,
      `PK route ${index} aggregator`,
    );
    equalAddress(
      await decryptionVerifier.circuitVerifier(),
      recorded.decryptionAggregatorVerifier,
      `decryption route ${index} aggregator`,
    );
    equalAddress(
      await decryptionVerifier.ciphernodeRegistry(),
      plan.registryProxy,
      `decryption route ${index} registry`,
    );
    const pkPaths = getBfvPkSubCircuitVkHashPaths(expected);
    const decryptionPaths = getBfvDecryptionSubCircuitVkHashPaths(expected);
    equalValue(
      pkRoute[2],
      readVkRecursiveHash(pkPaths.nodesFold, expected),
      `PK route ${index} nodes-fold VK`,
    );
    equalValue(
      pkRoute[3],
      readVkRecursiveHash(pkPaths.c5, expected),
      `PK route ${index} C5 VK`,
    );
    equalValue(
      decryptionRoute[2],
      readVkRecursiveHash(decryptionPaths.c6Fold, expected),
      `decryption route ${index} C6-fold VK`,
    );
    equalValue(
      decryptionRoute[3],
      readVkRecursiveHash(decryptionPaths.c7, expected),
      `decryption route ${index} C7 VK`,
    );
  }

  deployment.interfoldImplementation = plan.interfoldImplementation;
  deployment.interfoldLifecycle = plan.lifecycleLibrary;
  deployment.interfoldPricing = plan.pricingLibrary;
  deployment.pkVerifier = plan.pkVerifier;
  deployment.decryptionVerifier = plan.decryptionVerifier;
  deployment.ciphertextVerifier = plan.ciphertextVerifier;
  deployment.crispProgram = plan.crispProgram;
  deployment.bfvVerifierRoutes = plan.bfvVerifierRoutes;
  const first = plan.bfvVerifierRoutes[0];
  deployment.dkgAggregatorVerifier = first.dkgAggregatorVerifier;
  deployment.decryptionAggregatorVerifier = first.decryptionAggregatorVerifier;
  deployment.verifierZkTranscriptLib = first.verifierZkTranscriptLib;
  deployment.dkgVerifierRelationsLib = first.dkgVerifierRelationsLib;
  deployment.decryptionVerifierRelationsLib =
    first.decryptionVerifierRelationsLib;
  writeJson(deploymentFile, deployment);

  console.log(`
Secure CRISP activation validated
  crypto config:       ${PRODUCTION_BFV_CONFIG.configId}
  secure BFV routes:   ${plan.bfvVerifierRoutes.length}
  CRISP program:       ${plan.crispProgram}
  node protocol:       ${plan.nodeRelease.protocolVersion}
  requests paused:     true

Restart matching ciphernodes and validate them before resuming requests.
`);
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  validateSecureCrispUpgrade().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
