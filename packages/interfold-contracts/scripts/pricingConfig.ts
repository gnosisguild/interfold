// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import type { PricingConfig } from "./protocol/types";

/** Returns the pricing used by local deployments and contract tests. */
export function localPricingConfig(
  protocolTreasury: string,
): PricingConfig & Record<string, string> {
  return {
    keyGenFixedPerNode: "100000",
    keyGenPerEncryptionProof: "50000",
    coordinationPerPair: "10000",
    availabilityPerNodePerSec: "50",
    decryptionPerNode: "300000",
    publicationBase: "1000000",
    verificationPerProof: "5000",
    protocolTreasury,
    marginBps: "1000",
    protocolShareBps: "0",
    dkgUtilizationBps: "2500",
    computeUtilizationBps: "5000",
    decryptUtilizationBps: "2500",
    minCommitteeSize: "0",
    minThreshold: "0",
    randomnessFlatFee: "1000000",
  };
}

/** Returns a stable deployment-record key for one pricing configuration. */
export function pricingConfigFingerprint(config: PricingConfig): string {
  return JSON.stringify({
    keyGenFixedPerNode: BigInt(config.keyGenFixedPerNode).toString(),
    keyGenPerEncryptionProof: BigInt(
      config.keyGenPerEncryptionProof,
    ).toString(),
    coordinationPerPair: BigInt(config.coordinationPerPair).toString(),
    availabilityPerNodePerSec: BigInt(
      config.availabilityPerNodePerSec,
    ).toString(),
    decryptionPerNode: BigInt(config.decryptionPerNode).toString(),
    publicationBase: BigInt(config.publicationBase).toString(),
    verificationPerProof: BigInt(config.verificationPerProof).toString(),
    protocolTreasury: config.protocolTreasury.toLowerCase(),
    marginBps: BigInt(config.marginBps).toString(),
    protocolShareBps: BigInt(config.protocolShareBps).toString(),
    dkgUtilizationBps: BigInt(config.dkgUtilizationBps).toString(),
    computeUtilizationBps: BigInt(config.computeUtilizationBps).toString(),
    decryptUtilizationBps: BigInt(config.decryptUtilizationBps).toString(),
    minCommitteeSize: BigInt(config.minCommitteeSize).toString(),
    minThreshold: BigInt(config.minThreshold).toString(),
    randomnessFlatFee: BigInt(config.randomnessFlatFee).toString(),
  });
}
