// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
import { buildModule } from "@nomicfoundation/hardhat-ignition/modules";

export default buildModule("BondingRegistry", (m) => {
  const ticketToken = m.getParameter("ticketToken");
  const licenseToken = m.getParameter("licenseToken");
  const registry = m.getParameter("registry");
  const slashedFundsTreasury = m.getParameter("slashedFundsTreasury");
  const ticketPrice = m.getParameter("ticketPrice");
  const licenseRequiredBond = m.getParameter("licenseRequiredBond");
  const expectedTicketDecimals = m.getParameter("expectedTicketDecimals", 6);
  const expectedLicenseDecimals = m.getParameter("expectedLicenseDecimals", 18);
  const minTicketBalance = m.getParameter("minTicketBalance");
  const exitDelay = m.getParameter("exitDelay");
  const owner = m.getParameter("owner");

  const bondingAssetLib = m.library("BondingAssetLib");
  const bondingEligibilityLib = m.library("BondingEligibilityLib");
  const bondingSlashingLib = m.library("BondingSlashingLib");
  const bondingRegistrationLib = m.library("BondingRegistrationLib");
  const bondingOwnershipLib = m.library("BondingOwnershipLib");
  const bondingRegistryImpl = m.contract("BondingRegistry", [], {
    libraries: {
      BondingAssetLib: bondingAssetLib,
      BondingEligibilityLib: bondingEligibilityLib,
      BondingSlashingLib: bondingSlashingLib,
      BondingRegistrationLib: bondingRegistrationLib,
      BondingOwnershipLib: bondingOwnershipLib,
    },
  });

  const initData = m.encodeFunctionCall(bondingRegistryImpl, "initialize", [
    owner,
    {
      ticketToken,
      licenseToken,
      ticketPrice,
      licenseRequiredBond,
      expectedTicketDecimals,
      expectedLicenseDecimals,
    },
    registry,
    slashedFundsTreasury,
    minTicketBalance,
    exitDelay,
  ]);

  const bondingRegistry = m.contract("TransparentUpgradeableProxy", [
    bondingRegistryImpl,
    owner,
    initData,
  ]);

  return {
    bondingAssetLib,
    bondingEligibilityLib,
    bondingRegistry,
    bondingSlashingLib,
    bondingRegistrationLib,
    bondingOwnershipLib,
  };
}) as any;
