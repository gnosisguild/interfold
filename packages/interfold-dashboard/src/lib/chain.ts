// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
// Ethereum mainnet public client + contract addresses.
// Addresses sourced from deployments/manifest.json (networks.mainnet).
//
// ABIs are imported from the canonical typechain factories in
// @interfold/contracts so they cannot drift from the deployed contracts.

import { createPublicClient, http, type Address } from 'viem'
import { mainnet } from 'viem/chains'
import {
  BondingRegistry__factory,
  CiphernodeRegistryOwnable__factory,
  Faucet__factory,
  Interfold__factory,
  InterfoldTicketToken__factory,
  InterfoldToken__factory,
} from '@interfold/contracts/types'

// E3 lifecycle stages — mirrors the Solidity `IInterfold.E3Stage` enum exactly.
// Defined locally (rather than imported from @interfold/sdk) so the dashboard
// has no dependency on the SDK's Rust/Noir build chain when deploying.
export enum E3Stage {
  None = 0,
  Requested = 1,
  CommitteeFinalized = 2,
  KeyPublished = 3,
  CiphertextReady = 4,
  Complete = 5,
  Failed = 6,
}

// All deployment-specific values are env-overridable (see .env.example) so the
// dashboard can point at a different deployment without code changes. Defaults
// are the current mainnet deployment from deployments/manifest.json.
const env = ((import.meta as any).env ?? {}) as Record<string, string | undefined>
const envStr = (key: string, fallback: string): string => {
  const v = env[key]
  return v && v.trim() !== '' ? v.trim() : fallback
}

// The faucet is the one address that can be switched off, so it needs to tell an
// unset variable (use the default) from an explicitly empty one (disable). Every
// other setting is resolved with `envStr`, which folds empty into the fallback
// and so can never yield the disabled state.
// No faucet on mainnet — disabled unless a testnet deployment sets the env var.
const FAUCET_DEFAULT = ''
const faucetAddress = (): string => {
  const configured = env['VITE_FAUCET_ADDRESS']
  return configured === undefined ? FAUCET_DEFAULT : configured.trim()
}

const RPC_URL = envStr('VITE_MAINNET_RPC', 'https://ethereum-rpc.publicnode.com')

export const publicClient = createPublicClient({
  chain: mainnet,
  transport: http(RPC_URL, { batch: true }),
})

export const CONTRACTS = {
  Interfold: envStr('VITE_INTERFOLD_ADDRESS', '0x28cF63B459e6218C69EA97ea7D90541cf648c715') as Address,
  CiphernodeRegistry: envStr('VITE_CIPHERNODE_REGISTRY_ADDRESS', '0xC927A5B2d8F68697bC28C0670df05178c93df2d7') as Address,
  // CRISP is not deployed on mainnet yet; MockE3Program fills the slot so the
  // poll views resolve until a real CRISP deployment replaces it via env.
  CRISPProgram: envStr('VITE_CRISP_PROGRAM_ADDRESS', '0x4976E5E47852eFCe6851d35B95A1A2E19456F3D7') as Address,
  // Operator-guide contracts. The bonding registry is the only address the guide
  // needs hardcoded — the ciphernode bond token, ticket wrapper, and ticket underlying
  // are all read back from it at runtime so they cannot drift.
  BondingRegistry: envStr('VITE_BONDING_REGISTRY_ADDRESS', '0x0ec90465095C21830BEcED07e032809A2Bd2915F') as Address,
  // Testnet-only convenience faucet (FOLD + fee token). The zero address or an
  // empty string disables the faucet card in the operator guide.
  Faucet: faucetAddress() as Address,
}

// The chain the dashboard writes to. Reads use `publicClient`; the operator guide
// refuses to send a transaction unless the wallet is on this chain.
export const CHAIN = mainnet

// First block to scan from — lower bound for getLogs. This bounds queries against
// Interfold, CiphernodeRegistry and CRISPProgram, so it must be the earliest of
// those three deploy blocks: a later value would silently drop registry events
// emitted before Interfold was deployed. On mainnet all three share one block.
export const DEPLOY_BLOCK = BigInt(envStr('VITE_DEPLOY_BLOCK', '25786403'))

// E3 timeout windows (seconds), matching the deployment's timeoutConfig. Used to
// decide whether an E3 is still genuinely active vs. expired without completing.
export const TIMEOUTS = {
  computeWindow: Number(envStr('VITE_COMPUTE_WINDOW', '604800')),
  decryptionWindow: Number(envStr('VITE_DECRYPTION_WINDOW', '21600')),
}

export const interfoldAbi = Interfold__factory.abi
export const ciphernodeRegistryAbi = CiphernodeRegistryOwnable__factory.abi
export const bondingRegistryAbi = BondingRegistry__factory.abi
export const ticketTokenAbi = InterfoldTicketToken__factory.abi
export const ciphernodeBondTokenAbi = InterfoldToken__factory.abi
export const faucetAbi = Faucet__factory.abi
