// SPDX-License-Identifier: LGPL-3.0-only
import { ZERO } from "../constants";
import { safeTx } from "../safe";
import type {
  ProtocolConfigFile,
  ProtocolContracts,
  ProtocolInterfaces,
  SafeTransaction,
} from "../types";

export function appendTicketTxs(
  txs: SafeTransaction[],
  config: ProtocolConfigFile,
  c: ProtocolContracts,
  i: ProtocolInterfaces,
) {
  txs.push(
    safeTx(
      c.ticketToken,
      i.ticket.encodeFunctionData("setRegistry", [config.bondingRegistryProxy]),
    ),
    safeTx(
      config.bondingRegistryProxy,
      i.bonding.encodeFunctionData("setBondingAssetConfig", [
        {
          ticketToken: c.ticketToken,
          licenseToken: config.fold,
          ticketPrice: BigInt(config.bonding.ticketPrice),
          licenseRequiredBond: BigInt(config.bonding.licenseRequiredBond),
          expectedTicketDecimals: config.bonding.ticketTokenDecimals,
          expectedLicenseDecimals: config.bonding.licenseTokenDecimals,
        },
      ]),
    ),
  );
  if (config.ticketToken.lockRegistry) {
    txs.push(
      safeTx(c.ticketToken, i.ticket.encodeFunctionData("lockRegistry", [])),
    );
  }
}

export function appendSlashingTxs(
  txs: SafeTransaction[],
  config: ProtocolConfigFile,
  c: ProtocolContracts,
  i: ProtocolInterfaces,
) {
  txs.push(
    safeTx(
      c.slashingManager,
      i.slashing.encodeFunctionData("setInterfold", [c.interfold]),
    ),
    safeTx(
      c.slashingManager,
      i.slashing.encodeFunctionData("setBondingRegistry", [
        config.bondingRegistryProxy,
      ]),
    ),
    safeTx(
      c.slashingManager,
      i.slashing.encodeFunctionData("setCiphernodeRegistry", [
        c.ciphernodeRegistry,
      ]),
    ),
    safeTx(
      c.slashingManager,
      i.slashing.encodeFunctionData("setE3RefundManager", [c.e3RefundManager]),
    ),
  );
  if (config.slasher !== ZERO) {
    txs.push(
      safeTx(
        c.slashingManager,
        i.slashing.encodeFunctionData("addSlasher", [config.slasher]),
      ),
    );
  }
}
