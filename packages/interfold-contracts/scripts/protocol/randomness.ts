// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";

import { safeTx } from "./safe";
import type {
  ProtocolConfigFile,
  ProtocolContracts,
  ProtocolInterfaces,
  RandomnessConfig,
  SafeTransaction,
} from "./types";
import { deployedAddress } from "./values";

export const vrfCoordinatorInterface = new ethersLib.Interface([
  "function addConsumer(uint256 subId,address consumer)",
  "function getSubscription(uint256 subId) view returns (uint96 balance,uint96 nativeBalance,uint64 reqCount,address owner,address[] consumers)",
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
): Promise<void> {
  const randomness = requireRandomnessConfig(config);
  const coordinator = new ethersLib.Contract(
    randomness.coordinator,
    vrfCoordinatorInterface,
    ethers.provider,
  );
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
  if (balance === 0n) {
    throw new Error(
      `VRF subscription ${randomness.subscriptionId} has no ${
        randomness.nativePayment ? "native" : "LINK"
      } balance`,
    );
  }
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
