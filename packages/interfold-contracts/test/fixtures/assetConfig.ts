// SPDX-License-Identifier: LGPL-3.0-only
import type { AddressLike, BigNumberish } from "ethers";

import type { IInterfold, Interfold } from "../../types/contracts/Interfold";
import type { BondingRegistry } from "../../types/contracts/registry/BondingRegistry";

export async function currentPricingConfig(interfold: Interfold) {
  const pricing = await interfold.getPricingConfig();
  return {
    keyGenFixedPerNode: pricing.keyGenFixedPerNode,
    keyGenPerEncryptionProof: pricing.keyGenPerEncryptionProof,
    coordinationPerPair: pricing.coordinationPerPair,
    availabilityPerNodePerSec: pricing.availabilityPerNodePerSec,
    decryptionPerNode: pricing.decryptionPerNode,
    publicationBase: pricing.publicationBase,
    verificationPerProof: pricing.verificationPerProof,
    protocolTreasury: pricing.protocolTreasury,
    marginBps: pricing.marginBps,
    protocolShareBps: pricing.protocolShareBps,
    dkgUtilizationBps: pricing.dkgUtilizationBps,
    computeUtilizationBps: pricing.computeUtilizationBps,
    decryptUtilizationBps: pricing.decryptUtilizationBps,
    minCommitteeSize: pricing.minCommitteeSize,
    minThreshold: pricing.minThreshold,
    randomnessFlatFee: pricing.randomnessFlatFee,
  };
}

export async function setPricingConfig(
  interfold: Interfold,
  pricing: IInterfold.PricingConfigStruct,
) {
  return interfold.setFeeAssetConfig({
    token: await interfold.feeToken(),
    expectedDecimals: await interfold.feeTokenDecimals(),
    pricing,
  });
}

export async function setBondingAssetConfig(
  registry: BondingRegistry,
  overrides: Partial<{
    ticketToken: AddressLike;
    ciphernodeBondToken: AddressLike;
    ticketPrice: BigNumberish;
    requiredCiphernodeBond: BigNumberish;
    expectedTicketDecimals: BigNumberish;
    expectedCiphernodeBondDecimals: BigNumberish;
  }> = {},
) {
  return registry.setBondingAssetConfig({
    ticketToken: overrides.ticketToken ?? (await registry.getTicketToken()),
    ciphernodeBondToken:
      overrides.ciphernodeBondToken ??
      (await registry.getCiphernodeBondToken()),
    ticketPrice: overrides.ticketPrice ?? (await registry.ticketPrice()),
    requiredCiphernodeBond:
      overrides.requiredCiphernodeBond ??
      (await registry.requiredCiphernodeBond()),
    expectedTicketDecimals: overrides.expectedTicketDecimals ?? 6,
    expectedCiphernodeBondDecimals:
      overrides.expectedCiphernodeBondDecimals ?? 18,
  });
}
