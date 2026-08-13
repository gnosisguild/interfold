// SPDX-License-Identifier: LGPL-3.0-only
import { CiphernodeRegistryOwnable__factory as RegistryFactory } from "../../types";
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

  const ticketFactory = await ethers.getContractFactory("InterfoldTicketToken");
  const ticket = await ticketFactory.deploy(
    config.ticketUnderlyingToken,
    ADDRESS_ONE,
    config.safe,
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
    config.safe,
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
    config.safe,
    registryFactory.interface.encodeFunctionData("initialize", [
      config.safe,
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
    config.safe,
    interfoldFactory.interface.encodeFunctionData("initialize", [
      config.safe,
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
      config.e3Programs[0],
    ]),
  );

  const refundFactory = await ethers.getContractFactory("E3RefundManager");
  const refundImpl = await refundFactory.deploy();
  await refundImpl.waitForDeployment();
  const e3RefundManagerImplementation = await deployedAddress(refundImpl);
  const refundProxy = await deployProxy(
    ethers,
    e3RefundManagerImplementation,
    config.safe,
    refundFactory.interface.encodeFunctionData("initialize", [
      config.safe,
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

  const bondingFactory = await ethers.getContractFactory("BondingRegistry", {
    libraries: {
      BondingAssetLib: bondingAssetLib,
      BondingEligibilityLib: bondingEligibilityLib,
      BondingSlashingLib: bondingSlashingLib,
    },
  });
  const bondingImpl = await bondingFactory.deploy();
  await bondingImpl.waitForDeployment();

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
