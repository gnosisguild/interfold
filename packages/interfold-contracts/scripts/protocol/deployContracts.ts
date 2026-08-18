// SPDX-License-Identifier: LGPL-3.0-only
import { CiphernodeRegistryOwnable__factory as RegistryFactory } from "../../types";
import {
  BFV_DKG_H,
  BFV_THRESHOLD_T,
  getBfvDecryptionSubCircuitVkHashPaths,
  getBfvPkSubCircuitVkHashPaths,
  readVkRecursiveHash,
} from "../utils";
import { ADDRESS_ONE } from "./constants";
import { ensurePoseidonT3 } from "./poseidon";
import { deployProxy } from "./proxies";
import type { ProtocolConfigFile, ProtocolDeployResult } from "./types";
import { deployedAddress, pricingConfig, timeoutConfig } from "./values";

export async function deployProtocolContracts(
  ethers: any,
  operator: any,
  config: ProtocolConfigFile,
): Promise<ProtocolDeployResult> {
  const poseidonT3 = await ensurePoseidonT3(ethers);

  let initialE3Program = config.e3Programs[0];
  if (config.deployMockE3Program) {
    const programFactory = await ethers.getContractFactory("MockE3Program");
    const program = await programFactory.deploy();
    await program.waitForDeployment();
    initialE3Program = await deployedAddress(program);
  }

  const ticketFactory = await ethers.getContractFactory("InterfoldTicketToken");
  const ticket = await ticketFactory.deploy(
    config.ticketUnderlyingToken,
    ADDRESS_ONE,
    config.protocolOwner,
  );
  await ticket.waitForDeployment();
  const ticketToken = await deployedAddress(ticket);

  const slashingEvidenceFactory = await ethers.getContractFactory(
    "SlashingEvidenceLib",
  );
  const slashingEvidence = await slashingEvidenceFactory.deploy();
  await slashingEvidence.waitForDeployment();
  const slashingEvidenceLib = await deployedAddress(slashingEvidence);
  const slashingFactory = await ethers.getContractFactory("SlashingManager", {
    libraries: { SlashingEvidenceLib: slashingEvidenceLib },
  });
  const slashing = await slashingFactory.deploy(
    BigInt(config.slashing.initialDelay),
    config.protocolOwner,
  );
  await slashing.waitForDeployment();
  const slashingManager = await deployedAddress(slashing);

  const registrySortitionFactory = await ethers.getContractFactory(
    "RegistrySortitionLib",
  );
  const registrySortition = await registrySortitionFactory.deploy();
  await registrySortition.waitForDeployment();
  const registrySortitionLib = await deployedAddress(registrySortition);

  const registryFactory = await ethers.getContractFactory(
    RegistryFactory.abi,
    RegistryFactory.linkBytecode({
      "npm/poseidon-solidity@0.0.5/PoseidonT3.sol:PoseidonT3": poseidonT3,
      "project/contracts/lib/RegistrySortitionLib.sol:RegistrySortitionLib":
        registrySortitionLib,
    }),
    operator,
  );
  const registryImpl = await registryFactory.deploy();
  await registryImpl.waitForDeployment();
  const ciphernodeRegistryImplementation = await deployedAddress(registryImpl);
  const registryProxy = await deployProxy(
    ethers,
    ciphernodeRegistryImplementation,
    config.protocolOwner,
    registryFactory.interface.encodeFunctionData("initialize", [
      config.protocolOwner,
      BigInt(config.registry.sortitionSubmissionWindow),
    ]),
  );

  const pricingLibFactory = await ethers.getContractFactory("InterfoldPricing");
  const pricingLib = await pricingLibFactory.deploy();
  await pricingLib.waitForDeployment();
  const interfoldPricing = await deployedAddress(pricingLib);

  const lifecycleLibFactory =
    await ethers.getContractFactory("InterfoldLifecycle");
  const lifecycleLib = await lifecycleLibFactory.deploy();
  await lifecycleLib.waitForDeployment();
  const interfoldLifecycle = await deployedAddress(lifecycleLib);

  const interfoldFactory = await ethers.getContractFactory("Interfold", {
    libraries: {
      InterfoldLifecycle: interfoldLifecycle,
      InterfoldPricing: interfoldPricing,
    },
  });
  const interfoldImpl = await interfoldFactory.deploy();
  await interfoldImpl.waitForDeployment();
  const interfoldImplementation = await deployedAddress(interfoldImpl);
  const interfoldProxy = await deployProxy(
    ethers,
    interfoldImplementation,
    config.protocolOwner,
    interfoldFactory.interface.encodeFunctionData("initialize", [
      config.protocolOwner,
      registryProxy.proxy,
      config.bondingRegistryProxy,
      ADDRESS_ONE,
      {
        token: config.feeToken,
        expectedDecimals: config.feeTokenDecimals,
        pricing: pricingConfig(config.interfold.pricing),
      },
      BigInt(config.interfold.maxDuration),
      timeoutConfig(config.interfold.timeoutConfig),
      initialE3Program,
    ]),
  );

  const refundFactory = await ethers.getContractFactory("E3RefundManager");
  const refundImpl = await refundFactory.deploy();
  await refundImpl.waitForDeployment();
  const e3RefundManagerImplementation = await deployedAddress(refundImpl);
  const refundProxy = await deployProxy(
    ethers,
    e3RefundManagerImplementation,
    config.protocolOwner,
    refundFactory.interface.encodeFunctionData("initialize", [
      config.protocolOwner,
      interfoldProxy.proxy,
      config.protocolTreasury,
    ]),
  );

  const bondingAssetFactory =
    await ethers.getContractFactory("BondingAssetLib");
  const bondingAsset = await bondingAssetFactory.deploy();
  await bondingAsset.waitForDeployment();
  const bondingAssetLib = await deployedAddress(bondingAsset);

  const bondingEligibilityFactory = await ethers.getContractFactory(
    "BondingEligibilityLib",
  );
  const bondingEligibility = await bondingEligibilityFactory.deploy();
  await bondingEligibility.waitForDeployment();
  const bondingEligibilityLib = await deployedAddress(bondingEligibility);

  const bondingSlashingFactory =
    await ethers.getContractFactory("BondingSlashingLib");
  const bondingSlashing = await bondingSlashingFactory.deploy();
  await bondingSlashing.waitForDeployment();
  const bondingSlashingLib = await deployedAddress(bondingSlashing);

  const bondingRegistrationFactory = await ethers.getContractFactory(
    "BondingRegistrationLib",
  );
  const bondingRegistration = await bondingRegistrationFactory.deploy();
  await bondingRegistration.waitForDeployment();
  const bondingRegistrationLib = await deployedAddress(bondingRegistration);

  const bondingOwnershipFactory = await ethers.getContractFactory(
    "BondingOwnershipLib",
  );
  const bondingOwnership = await bondingOwnershipFactory.deploy();
  await bondingOwnership.waitForDeployment();
  const bondingOwnershipLib = await deployedAddress(bondingOwnership);

  const bondingFactory = await ethers.getContractFactory("BondingRegistry", {
    libraries: {
      BondingAssetLib: bondingAssetLib,
      BondingEligibilityLib: bondingEligibilityLib,
      BondingSlashingLib: bondingSlashingLib,
      BondingRegistrationLib: bondingRegistrationLib,
      BondingOwnershipLib: bondingOwnershipLib,
    },
  });
  const bondingImpl = await bondingFactory.deploy();
  await bondingImpl.waitForDeployment();

  // Bound to the proxy, not the implementation: the proxy is the address that calls `sync`, and
  // `BondedCheckpoints` accepts writes from exactly one address. The registry is pointed at this
  // contract by a `setBondedCheckpoints` transaction in the governance batch, after `initialize`.
  const checkpointsFactory =
    await ethers.getContractFactory("BondedCheckpoints");
  const checkpoints = await checkpointsFactory.deploy(
    config.bondingRegistryProxy,
  );
  await checkpoints.waitForDeployment();
  const bondedCheckpoints = await deployedAddress(checkpoints);

  const deployedVerifiers = config.verifiers?.deploy
    ? await deployBfvVerifiers(ethers, registryProxy.proxy)
    : {
        decryptionVerifier: config.verifiers?.decryptionVerifier,
        pkVerifier: config.verifiers?.pkVerifier,
        dkgFoldAttestationVerifier:
          config.verifiers?.dkgFoldAttestationVerifier,
      };

  // `BondedVotes` is deliberately NOT deployed here. Its constructor asks the registry which token
  // it bonds and refuses to build unless that matches the token it will read votes from — and the
  // registry is only initialized later, by the governance batch this script writes. It is deployed by
  // `--action activate-voting`, once that batch has executed.

  return {
    contracts: {
      ticketToken,
      slashingManager,
      slashingEvidenceLib,
      poseidonT3,
      registrySortitionLib,
      ciphernodeRegistry: registryProxy.proxy,
      ciphernodeRegistryImplementation,
      ciphernodeRegistryProxyAdmin: registryProxy.proxyAdmin,
      interfold: interfoldProxy.proxy,
      interfoldImplementation,
      interfoldProxyAdmin: interfoldProxy.proxyAdmin,
      interfoldLifecycle,
      interfoldPricing,
      e3RefundManager: refundProxy.proxy,
      e3RefundManagerImplementation,
      e3RefundManagerProxyAdmin: refundProxy.proxyAdmin,
      bondingAssetLib,
      bondingEligibilityLib,
      bondingRegistryImplementation: await deployedAddress(bondingImpl),
      bondingSlashingLib,
      bondingRegistrationLib,
      bondingOwnershipLib,
      bondedCheckpoints,
      initialE3Program,
      ...deployedVerifiers,
    },
    interfaces: {
      ticket: ticketFactory.interface,
      slashing: slashingFactory.interface,
      registry: registryFactory.interface,
      interfold: interfoldFactory.interface,
      bonding: bondingFactory.interface,
    },
  };
}

async function deployBfvVerifiers(ethers: any, registry: string) {
  const zkTranscriptFactory = await ethers.getContractFactory(
    "contracts/verifiers/bfv/honk/DkgAggregatorVerifier.sol:ZKTranscriptLib",
  );
  const zkTranscript = await zkTranscriptFactory.deploy();
  await zkTranscript.waitForDeployment();
  const verifierZkTranscriptLib = await deployedAddress(zkTranscript);

  const dkgRelationsFactory = await ethers.getContractFactory(
    "contracts/verifiers/bfv/honk/DkgAggregatorVerifier.sol:RelationsLib",
  );
  const dkgRelations = await dkgRelationsFactory.deploy();
  await dkgRelations.waitForDeployment();
  const dkgVerifierRelationsLib = await deployedAddress(dkgRelations);

  const decryptionRelationsFactory = await ethers.getContractFactory(
    "contracts/verifiers/bfv/honk/DecryptionAggregatorVerifier.sol:RelationsLib",
  );
  const decryptionRelations = await decryptionRelationsFactory.deploy();
  await decryptionRelations.waitForDeployment();
  const decryptionVerifierRelationsLib =
    await deployedAddress(decryptionRelations);

  const dkgAggregatorFactory = await ethers.getContractFactory(
    "contracts/verifiers/bfv/honk/DkgAggregatorVerifier.sol:DkgAggregatorVerifier",
    {
      libraries: {
        "project/contracts/verifiers/bfv/honk/DkgAggregatorVerifier.sol:ZKTranscriptLib":
          verifierZkTranscriptLib,
        "project/contracts/verifiers/bfv/honk/DkgAggregatorVerifier.sol:RelationsLib":
          dkgVerifierRelationsLib,
      },
    },
  );
  const dkgAggregator = await dkgAggregatorFactory.deploy();
  await dkgAggregator.waitForDeployment();
  const dkgAggregatorVerifier = await deployedAddress(dkgAggregator);

  const decryptionAggregatorFactory = await ethers.getContractFactory(
    "contracts/verifiers/bfv/honk/DecryptionAggregatorVerifier.sol:DecryptionAggregatorVerifier",
    {
      libraries: {
        "project/contracts/verifiers/bfv/honk/DecryptionAggregatorVerifier.sol:ZKTranscriptLib":
          verifierZkTranscriptLib,
        "project/contracts/verifiers/bfv/honk/DecryptionAggregatorVerifier.sol:RelationsLib":
          decryptionVerifierRelationsLib,
      },
    },
  );
  const decryptionAggregator = await decryptionAggregatorFactory.deploy();
  await decryptionAggregator.waitForDeployment();
  const decryptionAggregatorVerifier =
    await deployedAddress(decryptionAggregator);

  const pkPaths = getBfvPkSubCircuitVkHashPaths();
  const pkFactory = await ethers.getContractFactory("BfvPkVerifier");
  const pk = await pkFactory.deploy(
    dkgAggregatorVerifier,
    readVkRecursiveHash(pkPaths.nodesFold),
    readVkRecursiveHash(pkPaths.c5),
    BFV_DKG_H,
  );
  await pk.waitForDeployment();
  const pkVerifier = await deployedAddress(pk);

  const decryptionPaths = getBfvDecryptionSubCircuitVkHashPaths();
  const decryptionFactory = await ethers.getContractFactory(
    "BfvDecryptionVerifier",
  );
  const decryption = await decryptionFactory.deploy(
    decryptionAggregatorVerifier,
    registry,
    readVkRecursiveHash(decryptionPaths.c6Fold),
    readVkRecursiveHash(decryptionPaths.c7),
    BFV_THRESHOLD_T,
  );
  await decryption.waitForDeployment();
  const decryptionVerifier = await deployedAddress(decryption);

  const dkgFoldFactory = await ethers.getContractFactory(
    "DkgFoldAttestationVerifier",
  );
  const dkgFold = await dkgFoldFactory.deploy();
  await dkgFold.waitForDeployment();
  const dkgFoldAttestationVerifier = await deployedAddress(dkgFold);

  return {
    decryptionVerifier,
    pkVerifier,
    dkgFoldAttestationVerifier,
    dkgAggregatorVerifier,
    decryptionAggregatorVerifier,
    verifierZkTranscriptLib,
    dkgVerifierRelationsLib,
    decryptionVerifierRelationsLib,
  };
}
