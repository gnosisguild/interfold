// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import type { HardhatRuntimeEnvironment } from "hardhat/types/hre";

import { Interfold, Interfold__factory as InterfoldFactory } from "../../types";
import { localPricingConfig, pricingConfigFingerprint } from "../pricingConfig";
import type { PricingConfig } from "../protocol/types";
import { getProxyAdmin, verifyProxyAdminOwner } from "../proxy";
import { readDeploymentArgs, storeDeploymentArgs } from "../utils";

/**
 * Timeout configuration for E3 stages
 */
export interface E3TimeoutConfig {
  dkgWindow: number;
  computeWindow: number;
  decryptionWindow: number;
}

/**
 * The arguments for the deployAndSaveInterfold function
 */
export interface InterfoldArgs {
  owner?: string;
  maxDuration?: string;
  registry?: string;
  bondingRegistry?: string;
  e3RefundManager?: string;
  feeToken?: string;
  feeTokenDecimals?: number;
  timeoutConfig?: E3TimeoutConfig;
  pricingConfig?: PricingConfig;
  initialE3Program: string;
  hre: HardhatRuntimeEnvironment;
}

/**
 * Deploys the Interfold contract and saves the deployment arguments
 * @param param0 - The deployment arguments
 * @returns The deployed Interfold contract
 */
export const deployAndSaveInterfold = async ({
  owner,
  maxDuration,
  registry,
  bondingRegistry,
  e3RefundManager,
  feeToken,
  feeTokenDecimals = 6,
  timeoutConfig,
  pricingConfig: requestedPricingConfig,
  initialE3Program,
  hre,
}: InterfoldArgs): Promise<{ interfold: Interfold }> => {
  const { ethers } = await hre.network.connect();

  if ((await ethers.provider.getCode(initialE3Program)) === "0x") {
    throw new Error(
      `initialE3Program has no deployed code: ${initialE3Program}`,
    );
  }

  const [signer] = await ethers.getSigners();

  const chain = hre.globalOptions.network;
  const preDeployedArgs = readDeploymentArgs("Interfold", chain);
  const effectivePricingConfig = owner
    ? (requestedPricingConfig ?? localPricingConfig(owner))
    : undefined;
  const effectivePricingFingerprint = effectivePricingConfig
    ? pricingConfigFingerprint(effectivePricingConfig)
    : undefined;

  if (
    !owner ||
    !maxDuration ||
    !registry ||
    !bondingRegistry ||
    !e3RefundManager ||
    !feeToken ||
    !timeoutConfig ||
    (preDeployedArgs?.constructorArgs?.owner === owner &&
      preDeployedArgs?.constructorArgs?.maxDuration === maxDuration &&
      preDeployedArgs?.constructorArgs?.registry === registry &&
      preDeployedArgs?.constructorArgs?.bondingRegistry === bondingRegistry &&
      preDeployedArgs?.constructorArgs?.e3RefundManager === e3RefundManager &&
      preDeployedArgs?.constructorArgs?.feeToken === feeToken &&
      preDeployedArgs?.constructorArgs?.feeTokenDecimals ===
        feeTokenDecimals.toString() &&
      preDeployedArgs?.constructorArgs?.timeoutConfig ===
        JSON.stringify(timeoutConfig) &&
      preDeployedArgs?.constructorArgs?.pricingConfig ===
        effectivePricingFingerprint &&
      preDeployedArgs?.constructorArgs?.initialE3Program === initialE3Program)
  ) {
    if (!preDeployedArgs?.address) {
      throw new Error("Interfold address not found, it must be deployed first");
    }
    const interfoldContract = InterfoldFactory.connect(
      preDeployedArgs.address,
      signer,
    );
    return { interfold: interfoldContract };
  }
  if (!effectivePricingConfig || !effectivePricingFingerprint) {
    throw new Error("Interfold pricing configuration is required");
  }

  const pricingLibFactory = await ethers.getContractFactory(
    "InterfoldPricing",
    signer,
  );
  const pricingLib = await pricingLibFactory.deploy();
  await pricingLib.waitForDeployment();
  const pricingLibAddress = await pricingLib.getAddress();

  const lifecycleLibFactory = await ethers.getContractFactory(
    "InterfoldLifecycle",
    signer,
  );
  const lifecycleLib = await lifecycleLibFactory.deploy();
  await lifecycleLib.waitForDeployment();
  const lifecycleLibAddress = await lifecycleLib.getAddress();

  const interfoldFactory = await ethers.getContractFactory("Interfold", {
    signer,
    libraries: {
      InterfoldLifecycle: lifecycleLibAddress,
      InterfoldPricing: pricingLibAddress,
    },
  });

  const interfold = await interfoldFactory.deploy();
  await interfold.waitForDeployment();
  const blockNumber = await ethers.provider.getBlockNumber();
  const interfoldAddress = await interfold.getAddress();

  storeDeploymentArgs(
    { address: pricingLibAddress, blockNumber },
    "InterfoldPricing",
    chain,
  );
  storeDeploymentArgs(
    { address: lifecycleLibAddress, blockNumber },
    "InterfoldLifecycle",
    chain,
  );

  const initData = interfoldFactory.interface.encodeFunctionData("initialize", [
    owner,
    registry,
    bondingRegistry,
    e3RefundManager,
    {
      token: feeToken,
      expectedDecimals: feeTokenDecimals,
      pricing: effectivePricingConfig,
    },
    maxDuration,
    timeoutConfig,
    initialE3Program,
  ]);

  const ProxyCF = await ethers.getContractFactory(
    "TransparentUpgradeableProxy",
  );
  const proxy = await ProxyCF.deploy(interfoldAddress, owner, initData);
  await proxy.waitForDeployment();
  const proxyAddress = await proxy.getAddress();

  const proxyAdminAddress = await getProxyAdmin(ethers.provider, proxyAddress);

  storeDeploymentArgs(
    {
      constructorArgs: {
        owner,
        registry,
        bondingRegistry,
        e3RefundManager,
        feeToken,
        feeTokenDecimals: feeTokenDecimals.toString(),
        maxDuration,
        timeoutConfig: JSON.stringify(timeoutConfig),
        randomnessFlatFee: effectivePricingConfig.randomnessFlatFee,
        pricingConfig: effectivePricingFingerprint,
        initialE3Program,
      },
      libraries: {
        InterfoldLifecycle: lifecycleLibAddress,
        InterfoldPricing: pricingLibAddress,
      },
      proxyRecords: {
        initData,
        initialOwner: owner,
        proxyAddress,
        proxyAdminAddress,
        implementationAddress: interfoldAddress,
      },
      blockNumber,
      address: proxyAddress,
    },
    "Interfold",
    chain,
  );

  const interfoldContract = InterfoldFactory.connect(proxyAddress, signer);

  return { interfold: interfoldContract };
};

/**
 * Upgrades the Interfold implementation while keeping the same proxy address
 * @param param0 - The upgrade arguments
 * @returns The upgraded Interfold contract (same proxy address)
 */
export const upgradeAndSaveInterfold = async ({
  ownerAddress,
  hre,
}: {
  ownerAddress: string;
  hre: HardhatRuntimeEnvironment;
}): Promise<{ interfold: Interfold; implementationAddress: string }> => {
  const { ethers } = await hre.network.connect();
  const [signer] = await ethers.getSigners();
  const chain = hre.globalOptions.network;

  const preDeployedArgs = readDeploymentArgs("Interfold", chain);
  if (!preDeployedArgs?.address) {
    throw new Error(
      "Interfold proxy not found. Deploy first before upgrading.",
    );
  }

  const proxyAddress = preDeployedArgs.address;

  const autoProxyAdminAddress = await getProxyAdmin(
    ethers.provider,
    proxyAddress,
  );
  console.log("Auto-deployed ProxyAdmin address:", autoProxyAdminAddress);

  const pricingLibFactory = await ethers.getContractFactory(
    "InterfoldPricing",
    signer,
  );
  const pricingLib = await pricingLibFactory.deploy();
  await pricingLib.waitForDeployment();
  const pricingLibAddress = await pricingLib.getAddress();

  const lifecycleLibFactory = await ethers.getContractFactory(
    "InterfoldLifecycle",
    signer,
  );
  const lifecycleLib = await lifecycleLibFactory.deploy();
  await lifecycleLib.waitForDeployment();
  const lifecycleLibAddress = await lifecycleLib.getAddress();

  const interfoldFactory = await ethers.getContractFactory("Interfold", {
    signer,
    libraries: {
      InterfoldLifecycle: lifecycleLibAddress,
      InterfoldPricing: pricingLibAddress,
    },
  });

  const newImplementation = await interfoldFactory.deploy();
  await newImplementation.waitForDeployment();
  const newImplementationAddress = await newImplementation.getAddress();
  console.log("New Implementation Address:", newImplementationAddress);

  const blockNumber = await ethers.provider.getBlockNumber();
  storeDeploymentArgs(
    { address: pricingLibAddress, blockNumber },
    "InterfoldPricing",
    chain,
  );
  storeDeploymentArgs(
    { address: lifecycleLibAddress, blockNumber },
    "InterfoldLifecycle",
    chain,
  );

  const proxyAdmin = await ethers.getContractAt(
    "ProxyAdmin",
    autoProxyAdminAddress,
    signer,
  );
  await verifyProxyAdminOwner(proxyAdmin, ownerAddress);

  // TODO: Add init data if needed
  const initData = "0x";
  const upgradeTx = await proxyAdmin.upgradeAndCall(
    proxyAddress,
    newImplementationAddress,
    initData,
  );
  await upgradeTx.wait();

  const existingProxyRecords = preDeployedArgs.proxyRecords
    ? Object.fromEntries(
        Object.entries(preDeployedArgs.proxyRecords).filter(
          ([, value]) => value !== undefined,
        ),
      )
    : {};

  const proxyRecords: Record<string, string | string[]> = {
    ...existingProxyRecords,
    implementationAddress: newImplementationAddress,
  };

  if (initData !== "0x") {
    proxyRecords.initData = initData;
  }

  storeDeploymentArgs(
    {
      ...preDeployedArgs,
      libraries: {
        InterfoldLifecycle: lifecycleLibAddress,
        InterfoldPricing: pricingLibAddress,
      },
      proxyRecords,
    },
    "Interfold",
    chain,
  );

  const interfoldContract = InterfoldFactory.connect(proxyAddress, signer);
  return {
    interfold: interfoldContract,
    implementationAddress: newImplementationAddress,
  };
};
