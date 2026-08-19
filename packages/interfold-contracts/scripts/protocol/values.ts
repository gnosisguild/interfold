// SPDX-License-Identifier: LGPL-3.0-only
import { ethers as ethersLib } from "ethers";

import { arg } from "./cli";
import { ZERO, abi } from "./constants";
import { configPath, readJson } from "./files";
import type { PricingConfig, ProtocolConfigFile, TimeoutConfig } from "./types";

export function address(value: string, label: string): string {
  try {
    return ethersLib.getAddress(value);
  } catch {
    throw new Error(`${label} is not a valid address: ${value}`);
  }
}

export function optionalAddress(
  value: string | undefined,
  label: string,
): string | undefined {
  if (!value || value === ZERO) return undefined;
  return address(value, label);
}

export async function requireContract(
  provider: ethersLib.Provider,
  target: string,
  label: string,
): Promise<void> {
  const code = await provider.getCode(target);
  if (code === "0x") throw new Error(`${label} has no code: ${target}`);
}

export async function deployedAddress(contract: {
  target?: unknown;
  getAddress?: () => Promise<string>;
}): Promise<string> {
  if (typeof contract.target === "string")
    return address(contract.target, "contract");
  if (contract.getAddress)
    return address(await contract.getAddress(), "contract");
  throw new Error("Could not determine deployed contract address");
}

export function encodeBfvParams(params: {
  degree: bigint;
  plaintextModulus: bigint;
  moduli: readonly bigint[];
  error1Variance: string;
}): string {
  return abi.encode(
    [
      "tuple(uint256 degree,uint256 plaintext_modulus,uint256[] moduli,string error1_variance)",
    ],
    [
      [
        params.degree,
        params.plaintextModulus,
        [...params.moduli],
        params.error1Variance,
      ],
    ],
  );
}

export function timeoutConfig(config: TimeoutConfig) {
  return {
    dkgWindow: BigInt(config.dkgWindow),
    computeWindow: BigInt(config.computeWindow),
    decryptionWindow: BigInt(config.decryptionWindow),
  };
}

export function pricingConfig(config: PricingConfig) {
  return {
    keyGenFixedPerNode: BigInt(config.keyGenFixedPerNode),
    keyGenPerEncryptionProof: BigInt(config.keyGenPerEncryptionProof),
    coordinationPerPair: BigInt(config.coordinationPerPair),
    availabilityPerNodePerSec: BigInt(config.availabilityPerNodePerSec),
    decryptionPerNode: BigInt(config.decryptionPerNode),
    publicationBase: BigInt(config.publicationBase),
    verificationPerProof: BigInt(config.verificationPerProof),
    protocolTreasury: address(
      config.protocolTreasury,
      "interfold.pricing.protocolTreasury",
    ),
    marginBps: BigInt(config.marginBps),
    protocolShareBps: BigInt(config.protocolShareBps),
    dkgUtilizationBps: BigInt(config.dkgUtilizationBps),
    computeUtilizationBps: BigInt(config.computeUtilizationBps),
    decryptUtilizationBps: BigInt(config.decryptUtilizationBps),
    minCommitteeSize: BigInt(config.minCommitteeSize),
    minThreshold: BigInt(config.minThreshold),
  };
}

export function loadConfig(file = configPath()): ProtocolConfigFile {
  const config = readJson<ProtocolConfigFile>(file);
  if (
    !config.interfold ||
    typeof config.interfold.registerActiveBfvParamSet !== "boolean"
  ) {
    throw new Error(
      "interfold.registerActiveBfvParamSet is required and must be a boolean",
    );
  }
  if (typeof config.feeTokenDecimals !== "number") {
    throw new Error("feeTokenDecimals is required and must be a number");
  }
  if (typeof config.ticketUnderlyingToken !== "string") {
    throw new Error("ticketUnderlyingToken is required and must be a string");
  }
  applyAddressOverride(
    config,
    "protocolOwner",
    "protocol-owner",
    "PROTOCOL_OWNER",
  );
  if (!config.protocolOwner && config.safe) {
    config.protocolOwner = config.safe;
  }
  applyGovernanceOverride(config);
  applyAddressOverride(config, "fold", "fold", "FOLD_ADDRESS");
  applyAddressOverride(
    config,
    "escrowVotesAdapter",
    "escrow-votes-adapter",
    "ESCROW_VOTES_ADAPTER",
  );
  applyAddressOverride(
    config,
    "bondingRegistryProxy",
    "bonding-registry",
    "BONDING_REGISTRY",
  );
  applyAddressOverride(
    config,
    "bondingRegistryProxyAdmin",
    "bonding-registry-proxy-admin",
    "BONDING_REGISTRY_PROXY_ADMIN",
  );
  applyAddressOverride(config, "feeToken", "fee-token", "FEE_TOKEN");
  applyAddressOverride(
    config,
    "ticketUnderlyingToken",
    "ticket-underlying-token",
    "TICKET_UNDERLYING_TOKEN",
  );
  applyAddressOverride(
    config,
    "protocolTreasury",
    "protocol-treasury",
    "PROTOCOL_TREASURY",
  );
  applyAddressOverride(
    config,
    "slashedFundsTreasury",
    "slashed-funds-treasury",
    "SLASHED_FUNDS_TREASURY",
  );
  applyAddressOverride(config, "slasher", "slasher", "SLASHER_ADDRESS");
  if (config.interfold.pricing.protocolTreasury === ZERO) {
    config.interfold.pricing.protocolTreasury = config.protocolTreasury;
  }
  validateConfig(config);
  return config;
}

function applyAddressOverride(
  config: ProtocolConfigFile,
  key: keyof Pick<
    ProtocolConfigFile,
    | "safe"
    | "protocolOwner"
    | "fold"
    | "escrowVotesAdapter"
    | "bondingRegistryProxy"
    | "bondingRegistryProxyAdmin"
    | "feeToken"
    | "ticketUnderlyingToken"
    | "protocolTreasury"
    | "slashedFundsTreasury"
    | "slasher"
  >,
  cliName: string,
  envName: string,
): void {
  const override = arg(cliName) ?? process.env[envName];
  const current = config[key];
  if (override && (!current || current === ZERO)) {
    config[key] = override;
  }
}

function applyGovernanceOverride(config: ProtocolConfigFile): void {
  const adminPlugin =
    arg("aragon-admin-plugin") ?? process.env.ARAGON_ADMIN_PLUGIN;
  const proposerSafe = arg("governance-safe") ?? process.env.GOVERNANCE_SAFE;
  const proposalMetadata =
    arg("governance-proposal-metadata") ??
    process.env.GOVERNANCE_PROPOSAL_METADATA;
  if (!adminPlugin && !proposerSafe && !proposalMetadata) return;

  config.governance ??= {
    adminPlugin: ZERO,
    proposerSafe: ZERO,
  };
  if (adminPlugin && config.governance.adminPlugin === ZERO) {
    config.governance.adminPlugin = adminPlugin;
  }
  if (proposerSafe && config.governance.proposerSafe === ZERO) {
    config.governance.proposerSafe = proposerSafe;
  }
  if (proposalMetadata && !config.governance.proposalMetadata) {
    config.governance.proposalMetadata = proposalMetadata;
  }
}

function validateConfig(config: ProtocolConfigFile): void {
  if (!config.name) throw new Error("Config name is required");
  if (!/^[A-Za-z0-9_-]+$/.test(config.name)) {
    throw new Error(
      "Config name may only contain letters, numbers, underscores and hyphens",
    );
  }
  config.protocolOwner = address(config.protocolOwner, "protocolOwner");
  if (config.protocolOwner === ZERO) {
    throw new Error("protocolOwner must not be the zero address");
  }
  if (config.safe === ZERO) config.safe = undefined;
  if (config.safe) {
    config.safe = address(config.safe, "safe");
    if (config.safe !== config.protocolOwner) {
      throw new Error(
        "safe must equal protocolOwner when a Safe is configured",
      );
    }
  }
  if (config.safe && config.governance) {
    throw new Error("Configure either safe or governance, not both");
  }
  if (config.governance) {
    config.governance.adminPlugin = address(
      config.governance.adminPlugin,
      "governance.adminPlugin",
    );
    config.governance.proposerSafe = address(
      config.governance.proposerSafe,
      "governance.proposerSafe",
    );
    if (config.governance.adminPlugin === ZERO) {
      throw new Error("governance.adminPlugin must not be the zero address");
    }
    if (config.governance.proposerSafe === ZERO) {
      throw new Error("governance.proposerSafe must not be the zero address");
    }
    config.governance.proposalMetadata ??= "0x";
    if (!ethersLib.isHexString(config.governance.proposalMetadata)) {
      throw new Error("governance.proposalMetadata must be hex bytes");
    }
  }
  config.fold = address(config.fold, "fold");
  config.escrowVotesAdapter = optionalAddress(
    config.escrowVotesAdapter,
    "escrowVotesAdapter",
  );
  config.bondingRegistryProxy = address(
    config.bondingRegistryProxy,
    "bondingRegistryProxy",
  );
  config.bondingRegistryProxyAdmin = address(
    config.bondingRegistryProxyAdmin,
    "bondingRegistryProxyAdmin",
  );
  config.feeToken = address(config.feeToken, "feeToken");
  config.ticketUnderlyingToken = address(
    config.ticketUnderlyingToken,
    "ticketUnderlyingToken",
  );
  config.protocolTreasury = address(
    config.protocolTreasury,
    "protocolTreasury",
  );
  config.slashedFundsTreasury = address(
    config.slashedFundsTreasury,
    "slashedFundsTreasury",
  );
  if (config.slasher !== ZERO)
    config.slasher = address(config.slasher, "slasher");
  config.interfold.pricing.protocolTreasury = address(
    config.interfold.pricing.protocolTreasury,
    "interfold.pricing.protocolTreasury",
  );
  if (!Array.isArray(config.e3Programs) || config.e3Programs.length !== 1) {
    throw new Error("Exactly one initial E3 Program is required");
  }
  if (
    config.deployMockE3Program !== undefined &&
    typeof config.deployMockE3Program !== "boolean"
  ) {
    throw new Error("deployMockE3Program must be a boolean");
  }
  const initialE3Program = address(config.e3Programs[0], "e3Programs[0]");
  if (config.deployMockE3Program && initialE3Program !== ZERO) {
    throw new Error(
      "e3Programs[0] must be the zero address when deployMockE3Program is true",
    );
  }
  if (!config.deployMockE3Program && initialE3Program === ZERO) {
    throw new Error("e3Programs[0] must not be the zero address");
  }
  config.e3Programs = [initialE3Program];
  if (config.verifiers) {
    config.verifiers.decryptionVerifier = optionalAddress(
      config.verifiers.decryptionVerifier,
      "decryptionVerifier",
    );
    config.verifiers.pkVerifier = optionalAddress(
      config.verifiers.pkVerifier,
      "pkVerifier",
    );
    config.verifiers.dkgFoldAttestationVerifier = optionalAddress(
      config.verifiers.dkgFoldAttestationVerifier,
      "dkgFoldAttestationVerifier",
    );
  }
  const ciphertextVerifier = optionalAddress(
    config.ciphertextVerifier,
    "ciphertextVerifier",
  );
  config.ciphertextVerifier = ciphertextVerifier;
  if (
    config.deployMockCiphertextVerifier !== undefined &&
    typeof config.deployMockCiphertextVerifier !== "boolean"
  ) {
    throw new Error("deployMockCiphertextVerifier must be a boolean");
  }
  if (config.deployMockCiphertextVerifier && ciphertextVerifier) {
    throw new Error(
      "ciphertextVerifier must be omitted when deployMockCiphertextVerifier is true",
    );
  }
  if (
    config.bindInitialE3Program &&
    !ciphertextVerifier &&
    !config.deployMockCiphertextVerifier
  ) {
    throw new Error(
      "ciphertextVerifier is required when bindInitialE3Program is true",
    );
  }
  if (config.deployMockE3Program && config.bindInitialE3Program) {
    throw new Error(
      "bindInitialE3Program must be false when deployMockE3Program is true",
    );
  }
}
