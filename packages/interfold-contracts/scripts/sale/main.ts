// SPDX-License-Identifier: LGPL-3.0-only
import { actionPlan } from "./actions";
import { actionBidClaim } from "./bidClaim";
import { arg } from "./cli";
import {
  actionAcceptOwnership,
  actionDeploy,
  actionProposeSafe,
} from "./deploy";
import { actionFullTest } from "./fullTest";
import { actionPrepare } from "./prepare";
import { actionValidate } from "./validate";

function printHelp(): void {
  console.log(`
Interfold FOLD sale pipeline

One script, selected by --action:
  --action prepare      Deploy Safe-owned MockBondingRegistry proxy + sale deployer
  --action plan         Resolve schedule/economics and write the deploy plan
  --action deploy       Operator submits deploySaleWithLiquidityLauncher
  --action propose-safe Propose Safe activation: accept ownership + set claim source
  --action validate     Check FOLD/CCA/Safe invariants
  --action bid-claim    Submit a CCA bid, exit, and claim FOLD
  --action full-test    Self-contained Sepolia/local rehearsal

Common flags:
  --safe 0x...              Required for --action prepare unless SAFE_ADDRESS is set
  --config <file>           Defaults to packages/interfold-contracts/deploy/sale/<network>-sale.config.json
                           full resolved config; generated infra/plan files sit beside it
  --plan <file>             Optional plan path override
  --deployment <file>       Optional deployment path override
  --safe-transactions <file> Optional manual Safe fallback batch path
  --safe-builder <file>     Optional Safe Transaction Builder import path
  --liquidity-launcher 0x... Override LiquidityLauncher address
  --lbp-strategy 0x...      Override LBPStrategy address
  --reserved-token-amount-for-lp N  FOLD wei reserved for LP in LBP mode
  --lp-allocation-rate-mps N Percent of raised currency routed to LP, 1e7 = 100%
  --migration-delay-blocks N Blocks after auction end before LBP migrate() is allowed
  --pool-fee N              Uniswap v4 pool fee for LBP migration
  --pool-tick-spacing N     Uniswap v4 pool tick spacing for LBP migration
  --pool-hook 0x...         Optional v4 hook for LBP migration
  --presale-start T         Unix seconds or ISO time when pre-bids open
  --auction-start T         Unix seconds or ISO time when non-zero CCA issuance starts
  --auction-end T           Unix seconds or ISO time when the CCA closes
  --floor-price-eth-per-fold N  Human ETH/FOLD floor price, e.g. 0.000012
  --tick-spacing-percent-of-floor N  CCA price increment as % of floor (default 1)
  --lp-allocation-percent N Percent of raised ETH routed to LP (default 25)
  --predicate-registry 0x... Deploy a Safe-owned Predicate validation hook
  --predicate-policy-id x... Predicate policy/verification hash for that hook
  --predicate-hook 0x...     Use an already deployed validation hook
  --hook-data 0x...          Encoded Predicate attestation for --action bid-claim
  --auction-duration-blocks N  CCA length in blocks when not deriving blocks from timestamps (default 40)
  --cca-offset-seconds N    Seconds until FOLD CCA_START from now (default 86400 = 1 day)
  --cca-duration-seconds N  Seconds FOLD CCA lasts (default 604800 = 7 days)
  --cca-start-timestamp N   Unix seconds for FOLD CCA_START; also derives CCA blocks by default
  --cca-end-timestamp N     Unix seconds for FOLD CCA_END; also derives CCA blocks by default
  --auction-start-timestamp N  Unix seconds used only for CCA startBlock derivation
  --auction-end-timestamp N    Unix seconds used only for CCA endBlock derivation
  --derive-auction-blocks   Derive CCA blocks from the FOLD timestamps
  --seconds-per-block N     Block-time estimate for timestamp -> block conversion (default 12)

Examples:
  pnpm sale --network sepolia --action full-test
  pnpm sale --network sepolia --action prepare --config deploy/sale/sepolia-july-dry-run.config.json
  pnpm sale --network sepolia --action plan --config deploy/sale/sepolia-july-dry-run.config.json
  pnpm sale --network mainnet --action prepare --safe 0xSafe --config packages/interfold-contracts/deploy/sale/mainnet-sale.config.json
  pnpm sale --network mainnet --action plan --config packages/interfold-contracts/deploy/sale/mainnet-sale.config.json
  pnpm sale --network mainnet --action deploy --config packages/interfold-contracts/deploy/sale/mainnet-sale.config.json --propose-safe
  pnpm sale --network mainnet --action propose-safe --config packages/interfold-contracts/deploy/sale/mainnet-sale.config.json
`);
}

export async function main(): Promise<void> {
  const action = (arg("action") ?? "help").toLowerCase();
  if (action === "help") return printHelp();
  if (action === "prepare") return actionPrepare();
  if (action === "plan") return void (await actionPlan());
  if (action === "deploy") return actionDeploy();
  if (action === "propose-safe") return actionProposeSafe();
  if (action === "accept-ownership") return actionAcceptOwnership();
  if (action === "validate") return actionValidate();
  if (action === "bid-claim") return actionBidClaim();
  if (action === "full-test") return actionFullTest();
  throw new Error(`Unknown --action: ${action}`);
}
