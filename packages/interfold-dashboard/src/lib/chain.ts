// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.
// Sepolia public client + contract addresses.
// Addresses sourced from packages/interfold-contracts/deployed_contracts.json
// and examples/CRISP/packages/crisp-contracts/deployed_contracts.json.
//
// ABIs are imported from the canonical typechain factories in
// @interfold/contracts so they cannot drift from the deployed contracts.

import { createPublicClient, http, type Address } from 'viem'
import { sepolia } from 'viem/chains'
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
// are the current Sepolia deployment from deployed_contracts.json.
const env = ((import.meta as any).env ?? {}) as Record<string, string | undefined>
const envStr = (key: string, fallback: string): string => {
  const v = env[key]
  return v && v.trim() !== '' ? v.trim() : fallback
}

// The faucet is the one address that can be switched off, so it needs to tell an
// unset variable (use the default) from an explicitly empty one (disable). Every
// other setting is resolved with `envStr`, which folds empty into the fallback
// and so can never yield the disabled state.
const FAUCET_DEFAULT = '0xCb350D89ACf8FC1720e4BF2cF59B70f30F8D2DbA'
const faucetAddress = (): string => {
  const configured = env['VITE_FAUCET_ADDRESS']
  return configured === undefined ? FAUCET_DEFAULT : configured.trim()
}

const RPC_URL = envStr('VITE_SEPOLIA_RPC', 'https://ethereum-sepolia.publicnode.com')

export const publicClient = createPublicClient({
  chain: sepolia,
  transport: http(RPC_URL, { batch: true }),
})

export const CONTRACTS = {
  Interfold: envStr('VITE_INTERFOLD_ADDRESS', '0x38A8A686A420023568E995b57B4FBEA371555Ba7') as Address,
  CiphernodeRegistry: envStr('VITE_CIPHERNODE_REGISTRY_ADDRESS', '0xa639b9a7AB05B787fFE258735Cf9541152a0E610') as Address,
  CRISPProgram: envStr('VITE_CRISP_PROGRAM_ADDRESS', '0x8D9c914446451fdE7FC0fdBcF20573E878c3DE5a') as Address,
  // Operator-guide contracts. The bonding registry is the only address the guide
  // needs hardcoded — the ciphernode bond token, ticket wrapper, and ticket underlying
  // are all read back from it at runtime so they cannot drift.
  BondingRegistry: envStr('VITE_BONDING_REGISTRY_ADDRESS', '0x4b8560271CD78e071f0074f6Db7D344298187F8b') as Address,
  // Testnet-only convenience faucet (FOLD + fee token). The zero address or an
  // empty string disables the faucet card in the operator guide.
  Faucet: faucetAddress() as Address,
}

// The chain the dashboard writes to. Reads use `publicClient`; the operator guide
// refuses to send a transaction unless the wallet is on this chain.
export const CHAIN = sepolia

// First block to scan from — lower bound for getLogs. This bounds queries against
// Interfold, CiphernodeRegistry and CRISPProgram, so it must be the earliest of
// those three deploy blocks (CiphernodeRegistry), not Interfold's: a later value
// would silently drop registry events emitted before Interfold was deployed.
export const DEPLOY_BLOCK = BigInt(envStr('VITE_DEPLOY_BLOCK', '11508403'))

// E3 timeout windows (seconds), matching the deployment's timeoutConfig. Used to
// decide whether an E3 is still genuinely active vs. expired without completing.
export const TIMEOUTS = {
  computeWindow: Number(envStr('VITE_COMPUTE_WINDOW', '86400')),
  decryptionWindow: Number(envStr('VITE_DECRYPTION_WINDOW', '3600')),
}

export const interfoldAbi = Interfold__factory.abi
export const ciphernodeRegistryAbi = CiphernodeRegistryOwnable__factory.abi
export const bondingRegistryAbi = BondingRegistry__factory.abi
export const ticketTokenAbi = InterfoldTicketToken__factory.abi
export const ciphernodeBondTokenAbi = InterfoldToken__factory.abi
export const faucetAbi = Faucet__factory.abi
