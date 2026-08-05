// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
// Shared operator setup helpers for sortition-based tests.
import type { Signer } from "ethers";

import { ethers } from "./connection";

/**
 * Register an operator for sortition: mint license, bond, register,
 * fund ticket balance, and add to the ciphernode registry.
 */
export async function setupOperatorForSortition(
  operator: Signer,
  bondOwner: Signer,
  bondingRegistry: any,
  licenseToken: any,
  usdcToken: any,
  ticketToken: any,
  registry: any,
): Promise<void> {
  const operatorAddress = await operator.getAddress();
  const bondOwnerAddress = await bondOwner.getAddress();

  await licenseToken.mint(
    bondOwnerAddress,
    ethers.parseEther("10000"),
    ethers.encodeBytes32String("Test allocation"),
  );
  await usdcToken.mint(bondOwnerAddress, ethers.parseUnits("100000", 6));

  await bondingRegistry.connect(operator).setBondOwner(bondOwnerAddress);
  await licenseToken
    .connect(bondOwner)
    .approve(await bondingRegistry.getAddress(), ethers.parseEther("2000"));
  await bondingRegistry
    .connect(bondOwner)
    .bondLicenseFor(operatorAddress, ethers.parseEther("1000"));
  await bondingRegistry.connect(bondOwner).registerOperatorFor(operatorAddress);

  const ticketAmount = ethers.parseUnits("100", 6);
  await usdcToken
    .connect(bondOwner)
    .approve(await ticketToken.getAddress(), ticketAmount);
  await bondingRegistry
    .connect(bondOwner)
    .addTicketBalanceFor(operatorAddress, ticketAmount);

  await registry.addCiphernode(operatorAddress);
}
