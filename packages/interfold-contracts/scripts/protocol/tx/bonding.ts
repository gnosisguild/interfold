// SPDX-License-Identifier: LGPL-3.0-only
import { proxyAdminInterface } from "../constants";
import { safeTx } from "../safe";
import type {
  ProtocolConfigFile,
  ProtocolContracts,
  ProtocolInterfaces,
  SafeTransaction,
} from "../types";

export function bondingUpgradeTx(
  config: ProtocolConfigFile,
  contracts: ProtocolContracts,
  interfaces: ProtocolInterfaces,
): SafeTransaction {
  return safeTx(
    config.bondingRegistryProxyAdmin,
    proxyAdminInterface.encodeFunctionData("upgradeAndCall", [
      config.bondingRegistryProxy,
      contracts.bondingRegistryImplementation,
      bondingInitData(config, contracts, interfaces),
    ]),
  );
}

export function appendBondingTxs(
  txs: SafeTransaction[],
  config: ProtocolConfigFile,
  c: ProtocolContracts,
  i: ProtocolInterfaces,
) {
  txs.push(
    safeTx(
      config.bondingRegistryProxy,
      i.bonding.encodeFunctionData("setSlashingManager", [c.slashingManager]),
    ),
    // `setRewardDistributor` authorizes; revoking is a separate call. Passing a second argument
    // made `encodeFunctionData` throw, so the batch could not be built at all.
    safeTx(
      config.bondingRegistryProxy,
      i.bonding.encodeFunctionData("setRewardDistributor", [c.interfold]),
    ),
    safeTx(
      config.bondingRegistryProxy,
      i.bonding.encodeFunctionData("setRewardDistributor", [c.e3RefundManager]),
    ),
    // Without this the bonded half of governance weight is never recorded: the sync is a no-op
    // while unconfigured, so every operator reads as zero bonded voting power.
    safeTx(
      config.bondingRegistryProxy,
      i.bonding.encodeFunctionData("setBondedCheckpoints", [
        c.bondedCheckpoints,
      ]),
    ),
  );
}

function bondingInitData(
  config: ProtocolConfigFile,
  c: ProtocolContracts,
  i: ProtocolInterfaces,
): string {
  return i.bonding.encodeFunctionData("initialize", [
    config.safe,
    {
      ticketToken: c.ticketToken,
      ciphernodeBondToken: config.fold,
      ticketPrice: BigInt(config.bonding.ticketPrice),
      requiredCiphernodeBond: BigInt(config.bonding.requiredCiphernodeBond),
      expectedTicketDecimals: config.bonding.ticketTokenDecimals,
      expectedCiphernodeBondDecimals:
        config.bonding.ciphernodeBondTokenDecimals,
    },
    c.ciphernodeRegistry,
    config.slashedFundsTreasury,
    BigInt(config.bonding.minTicketBalance),
    BigInt(config.bonding.exitDelay),
  ]);
}
