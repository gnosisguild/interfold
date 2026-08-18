// SPDX-License-Identifier: LGPL-3.0-only
import {
  actionDeploy,
  actionExecuteGovernance,
  actionProposeSafe,
} from "./actions";
import { actionActivateVoting } from "./activateVoting";
import { arg } from "./cli";
import { actionPrepareRehearsal } from "./prepareRehearsal";
import { actionValidate } from "./validate";

function printHelp(): void {
  console.log(`
Interfold protocol deployment

Actions:
  --action deploy       Deploy protocol contracts and write one governance wiring batch
  --action propose-safe Propose the written governance batch through the Safe SDK
  --action execute-governance  Execute wiring directly on a non-mainnet rehearsal chain
  --action prepare-rehearsal   Deploy fresh Sepolia prerequisites and write the rehearsal config
  --action activate-voting  Deploy BondedVotes once governance has configured the registry
  --action validate     Validate after the governance batch executes

Examples:
  pnpm protocol --network sepolia --action deploy --config packages/interfold-contracts/deploy/protocol/sepolia-protocol.config.json
  pnpm protocol --network sepolia --action propose-safe --config packages/interfold-contracts/deploy/protocol/sepolia-protocol.config.json
  pnpm protocol --network sepolia --action validate --config packages/interfold-contracts/deploy/protocol/sepolia-protocol.config.json

Flags:
  --sync-integration-config  Also update tests/integration/interfold.config.yaml
  --protocol-owner 0x...     Fill a zero protocol-owner placeholder
  --fold 0x...               Fill a zero FOLD placeholder
  --bonding-registry 0x...   Fill a zero BondingRegistry proxy placeholder
  --bonding-registry-proxy-admin 0x...
  --fee-token 0x...
  --ticket-underlying-token 0x...
  --protocol-treasury 0x...
  --slashed-funds-treasury 0x...
  --slasher 0x...
`);
}

export async function main(): Promise<void> {
  const action = (arg("action") ?? "help").toLowerCase();
  if (action === "help") return printHelp();
  if (action === "deploy") return actionDeploy();
  if (action === "propose-safe") return actionProposeSafe();
  if (action === "execute-governance") return actionExecuteGovernance();
  if (action === "prepare-rehearsal") return actionPrepareRehearsal();
  if (action === "activate-voting") return actionActivateVoting();
  if (action === "validate") return actionValidate();
  throw new Error(`Unknown --action: ${action}`);
}
