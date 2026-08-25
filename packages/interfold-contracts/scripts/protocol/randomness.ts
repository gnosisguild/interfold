// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";

import { safeTx } from "./safe";
import type {
  ProtocolConfigFile,
  ProtocolContracts,
  ProtocolDeployment,
  ProtocolInterfaces,
  RandomnessConfig,
  SafeTransaction,
  VrfSortitionUpgradePlan,
} from "./types";
import { deployedAddress } from "./values";

export const vrfCoordinatorInterface = new ethersLib.Interface([
  "function addConsumer(uint256 subId,address consumer)",
  "function getSubscription(uint256 subId) view returns (uint96 balance,uint96 nativeBalance,uint64 reqCount,address owner,address[] consumers)",
  "function s_config() view returns (uint16 minimumRequestConfirmations,uint32 maxGasLimit,bool reentrancyLock,uint32 stalenessSeconds,uint32 gasAfterPaymentCalculation,uint32 fulfillmentFlatFeeNativePPM,uint32 fulfillmentFlatFeeLinkDiscountPPM,uint8 nativePremiumPercentage,uint8 linkPremiumPercentage)",
  "function s_provingKeys(bytes32 keyHash) view returns (bool exists,uint64 maxGas)",
]);

export const vrfProviderInterface = new ethersLib.Interface([
  "function acceptOwnership()",
]);

export function requireRandomnessConfig(
  config: ProtocolConfigFile,
): RandomnessConfig {
  if (!config.randomness) {
    throw new Error("randomness configuration is required");
  }
  if (BigInt(config.randomness.subscriptionId) === 0n) {
    throw new Error("randomness.subscriptionId must not be zero");
  }
  return config.randomness;
}

export async function assertVrfSubscription(
  ethers: any,
  config: ProtocolConfigFile,
  expectedConsumer?: string,
): Promise<void> {
  const randomness = requireRandomnessConfig(config);
  const coordinator = new ethersLib.Contract(
    randomness.coordinator,
    vrfCoordinatorInterface,
    ethers.provider,
  );
  const coordinatorConfig = await coordinator.s_config();
  if (
    BigInt(randomness.requestConfirmations) <
    BigInt(coordinatorConfig.minimumRequestConfirmations)
  ) {
    throw new Error(
      `randomness.requestConfirmations is below the coordinator minimum of ${coordinatorConfig.minimumRequestConfirmations}`,
    );
  }
  if (
    BigInt(randomness.callbackGasLimit) > BigInt(coordinatorConfig.maxGasLimit)
  ) {
    throw new Error(
      `randomness.callbackGasLimit exceeds the coordinator maximum of ${coordinatorConfig.maxGasLimit}`,
    );
  }
  const provingKey = await coordinator.s_provingKeys(randomness.keyHash);
  if (!provingKey.exists) {
    throw new Error(
      `randomness.keyHash is not registered with coordinator ${randomness.coordinator}`,
    );
  }
  const subscription = await coordinator.getSubscription(
    BigInt(randomness.subscriptionId),
  );
  if (
    String(subscription.owner).toLowerCase() !==
    config.protocolOwner.toLowerCase()
  ) {
    throw new Error(
      `VRF subscription owner mismatch: expected ${config.protocolOwner}, got ${subscription.owner}`,
    );
  }
  const balance = randomness.nativePayment
    ? subscription.nativeBalance
    : subscription.balance;
  const minimumBalance = BigInt(randomness.minimumSubscriptionBalance);
  if (balance < minimumBalance) {
    throw new Error(
      `VRF subscription ${randomness.subscriptionId} ${
        randomness.nativePayment ? "native" : "LINK"
      } balance ${balance} is below the configured minimum ${minimumBalance}`,
    );
  }
  if (
    expectedConsumer &&
    !subscription.consumers.some(
      (consumer: string) =>
        consumer.toLowerCase() === expectedConsumer.toLowerCase(),
    )
  ) {
    throw new Error(
      `VRF subscription does not include consumer ${expectedConsumer}`,
    );
  }
}

function assertPlanValue(
  label: string,
  actual: string | number,
  expected: string | number,
): void {
  if (String(actual).toLowerCase() !== String(expected).toLowerCase()) {
    throw new Error(`${label}: expected ${expected}, got ${actual}`);
  }
}

export function assertVrfUpgradePlanMatchesDeployment(
  config: ProtocolConfigFile,
  deployment: ProtocolDeployment,
  plan: VrfSortitionUpgradePlan,
  connectedChainId: bigint,
): void {
  assertPlanValue(
    "connected chain",
    connectedChainId.toString(),
    config.chainId,
  );
  assertPlanValue("deployment chain", deployment.chainId, config.chainId);
  assertPlanValue("plan name", plan.name, config.name);
  assertPlanValue(
    "plan protocol owner",
    plan.protocolOwner,
    config.protocolOwner,
  );
  assertPlanValue(
    "deployment protocol owner",
    deployment.protocolOwner,
    config.protocolOwner,
  );
  assertPlanValue(
    "registry proxy",
    plan.registryProxy,
    deployment.ciphernodeRegistry,
  );
  assertPlanValue(
    "registry ProxyAdmin",
    plan.registryProxyAdmin,
    deployment.ciphernodeRegistryProxyAdmin,
  );
  assertPlanValue("Interfold proxy", plan.interfoldProxy, deployment.interfold);
  assertPlanValue(
    "Interfold ProxyAdmin",
    plan.interfoldProxyAdmin,
    deployment.interfoldProxyAdmin,
  );
}

export async function deployRandomnessProvider(
  ethers: any,
  operator: any,
  config: ProtocolConfigFile,
  registry: string,
): Promise<{
  randomnessProvider: string;
  randomnessProviderOwnershipAcceptanceRequired: boolean;
}> {
  const randomness = requireRandomnessConfig(config);
  const factory = await ethers.getContractFactory(
    "ChainlinkVrfRandomnessProvider",
  );
  const provider = await factory.deploy(
    registry,
    randomness.coordinator,
    BigInt(randomness.subscriptionId),
    randomness.keyHash,
    randomness.requestConfirmations,
    randomness.callbackGasLimit,
    randomness.nativePayment,
    config.protocolOwner,
  );
  await provider.waitForDeployment();
  return {
    randomnessProvider: await deployedAddress(provider),
    randomnessProviderOwnershipAcceptanceRequired:
      (await operator.getAddress()).toLowerCase() !==
      config.protocolOwner.toLowerCase(),
  };
}

export function appendRandomnessTxs(
  txs: SafeTransaction[],
  config: ProtocolConfigFile,
  contracts: ProtocolContracts,
  interfaces: ProtocolInterfaces,
): void {
  txs.push(
    ...buildRandomnessTransactions(
      config,
      contracts.randomnessProvider,
      contracts.ciphernodeRegistry,
      interfaces.registry,
      Boolean(contracts.randomnessProviderOwnershipAcceptanceRequired),
    ),
  );
}

export function buildRandomnessTransactions(
  config: ProtocolConfigFile,
  randomnessProvider: string,
  registry: string,
  registryInterface: ProtocolInterfaces["registry"],
  acceptProviderOwnership: boolean,
): SafeTransaction[] {
  const randomness = requireRandomnessConfig(config);
  const txs: SafeTransaction[] = [];
  if (acceptProviderOwnership) {
    txs.push(
      safeTx(
        randomnessProvider,
        vrfProviderInterface.encodeFunctionData("acceptOwnership"),
      ),
    );
  }
  txs.push(
    safeTx(
      randomness.coordinator,
      vrfCoordinatorInterface.encodeFunctionData("addConsumer", [
        BigInt(randomness.subscriptionId),
        randomnessProvider,
      ]),
    ),
    safeTx(
      registry,
      registryInterface.encodeFunctionData("setRandomnessRequestTimeout", [
        BigInt(randomness.requestTimeout),
      ]),
    ),
    safeTx(
      registry,
      registryInterface.encodeFunctionData("setRandomnessProvider", [
        randomnessProvider,
      ]),
    ),
  );
  return txs;
}
