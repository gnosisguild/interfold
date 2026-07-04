# Mainnet CCA Deployment Checklist

This is the deployment-only checklist for the mainnet FOLD + Uniswap CCA launch.
Do not use it as the post-auction migration, exit, or claim checklist.

## Mainnet Addresses

Confirm these before touching mainnet:

- [ ] Foundation Safe / owner / unsold token recipient / proceeds recipient / LP
      position recipient: `0x5429D8c7fD14023f3c414126F94BbE25A05fC018`
- [ ] ETH currency sentinel: `0x0000000000000000000000000000000000000000`
- [ ] Uniswap LiquidityLauncher v3.0.0:
      `0x00004c4ccc709Ef590F7C81102C0689F0263D4e9`
- [ ] Uniswap LBPStrategy v3.0.0 mainnet:
      `0xb98766A35cdc28415be0767D4EA41e39fBA3e000`
- [ ] Uniswap CCA factory v2.0.0: `0x00cCa200BF124dBfA848937c553864f4B4CE0632`
- [ ] Predicate registry mainnet matches the Predicate dashboard and
      `PREDICATE_REGISTRY`.
- [ ] Predicate policy ID matches the mainnet Application Compliance dashboard
      verification hash.
- [ ] Operator / deployer wallet address is written down and added as a Safe
      proposer.

## 1. Safe And Operator

- [ ] Foundation Safe exists.
- [ ] Foundation Safe address is exactly
      `0x5429D8c7fD14023f3c414126F94BbE25A05fC018`.
- [ ] Operator wallet is funded with mainnet ETH for gas only.
- [ ] Operator wallet is added as a Safe proposer.
- [ ] Operator proposer access has been tested in the Safe UI before launch.
- [ ] Operator wallet is not a Safe signer unless intentionally approved.
- [ ] Safe owners are online and know they must approve and execute the
      activation batch immediately after deployment.

## 2. Environment

- [ ] `PRIVATE_KEY` is the operator wallet.
- [ ] `SAFE_ADDRESS=0x5429D8c7fD14023f3c414126F94BbE25A05fC018`.
- [ ] `SAFE_API_KEY` is set.
- [ ] `RPC_URL` points to Ethereum mainnet.
- [ ] `ETHERSCAN_API_KEY` is set.
- [ ] If Predicate is enabled, `PREDICATE_REGISTRY` is the mainnet Predicate
      registry from the dashboard.
- [ ] If Predicate is enabled, `PREDICATE_POLICY_ID` is the mainnet policy
      verification hash.
- [ ] If Predicate is enabled,
      `PREDICATE_ATTESTATION_URL=https://api.predicate.io/v2/attestation`.
- [ ] If Predicate is enabled, `PREDICATE_CHAIN=ethereum`.
- [ ] `PREDICATE_API_KEY` is present only in backend/operator env, not
      committed.

## 3. Config

Config file:

```bash
packages/interfold-contracts/deploy/sale/mainnet-sale.config.json
```

Required values:

- [ ] `name = interfold-mainnet-cca`.
- [ ] `chainId = 1`.
- [ ] `safe = 0x5429D8c7fD14023f3c414126F94BbE25A05fC018`.
- [ ] `launchMode = lbp`.
- [ ] `saleAmountFold = 120000000`.
- [ ] `auction.currency = ETH`.
- [ ] `auction.tokensRecipient = 0x5429D8c7fD14023f3c414126F94BbE25A05fC018`.
- [ ] `auction.fundsRecipient = 0xb98766A35cdc28415be0767D4EA41e39fBA3e000`.
- [ ] `auction.floorPriceEthPerFold = 0.000012`.
- [ ] `auction.requiredRaiseEth = 400`.
- [ ] `auction.tickSpacingPercentOfFloor = 1`.
- [ ] `auction.preSaleStartTimestamp = 2026-07-06T10:00:00-04:00`.
- [ ] `auction.auctionStartTimestamp = 2026-07-08T10:00:00-04:00`.
- [ ] `auction.auctionEndTimestamp = 2026-07-10T10:00:00-04:00`.
- [ ] If Predicate is enabled, `auction.validationHook` is filled by `prepare`.
- [ ] `lbp.uniswap.liquidityLauncher` is
      `0x00004c4ccc709Ef590F7C81102C0689F0263D4e9`.
- [ ] `lbp.uniswap.lbpStrategy` is `0xb98766A35cdc28415be0767D4EA41e39fBA3e000`.
- [ ] `lbp.lpAllocationPercent = 25`.
- [ ] `lbp.migrationDelayBlocks = 20`.
- [ ] `lbp.recipients.proceedsRecipient` is
      `0x5429D8c7fD14023f3c414126F94BbE25A05fC018`.
- [ ] `lbp.recipients.lpPositionRecipient` is
      `0x5429D8c7fD14023f3c414126F94BbE25A05fC018`.
- [ ] `lbp.pool.fee = 3000`.
- [ ] `lbp.pool.tickSpacing = 60`.
- [ ] `lbp.pool.hook = 0x0000000000000000000000000000000000000000`.
- [ ] `lbp.advanced.positionDefinitions` is the empty array encoding.

## 4. Time Model

- [ ] FOLD lifecycle uses timestamps.
- [ ] CCA auction mechanics use block numbers.
- [ ] `plan` derives CCA block numbers from configured timestamps.
- [ ] Pre-sale / FOLD `CCA_START`: July 6, 2026 at 10:00 AM US Eastern.
- [ ] Auction start: July 8, 2026 at 10:00 AM US Eastern.
- [ ] Auction end: July 10, 2026 at 10:00 AM US Eastern.
- [ ] UTC pre-sale / FOLD `CCA_START`: July 6, 2026 at 14:00 UTC.
- [ ] UTC auction start: July 8, 2026 at 14:00 UTC.
- [ ] UTC auction end: July 10, 2026 at 14:00 UTC.
- [ ] `NO_MORE_LOCKS` is `CCA_END + 40 days + 4 years + 30 days`.
- [ ] Expected mainnet `NO_MORE_LOCKS` is September 17, 2030 at 14:00 UTC.
- [ ] `claimBlock = migrationBlock`.
- [ ] Plan output shows `migrationBlock >= endBlock`.

## 5. Prepare

Run:

```bash
pnpm sale --network mainnet --action prepare \
  --config ./deploy/sale/mainnet-sale.config.json \
  --predicate-registry "$PREDICATE_REGISTRY" \
  --predicate-policy-id "$PREDICATE_POLICY_ID"
```

If Predicate is not enabled, omit `--predicate-registry` and
`--predicate-policy-id`.

Check:

- [ ] Config file is written under `packages/interfold-contracts/deploy/sale`.
- [ ] Infra file is written under `packages/interfold-contracts/deploy/sale`.
- [ ] `saleDeployer.protocolAdmin = Foundation Safe`.
- [ ] MockBondingRegistry implementation is deployed.
- [ ] BondingRegistry proxy is deployed.
- [ ] BondingRegistry ProxyAdmin owner is Foundation Safe.
- [ ] Sale deployer is deployed.
- [ ] If Predicate is enabled, PredicateValidationHook owner is Foundation Safe.
- [ ] If Predicate is enabled, PredicateValidationHook policy ID equals
      dashboard verification hash.
- [ ] If Predicate is enabled, add the PredicateValidationHook address to the
      Predicate Application Compliance dashboard.
- [ ] If Predicate is enabled, dashboard accepts the hook contract.

## 6. Plan

Run:

```bash
pnpm sale --network mainnet --action plan \
  --config ./deploy/sale/mainnet-sale.config.json
```

Check every printed line:

- [ ] Chain ID is `1`.
- [ ] Safe is `0x5429D8c7fD14023f3c414126F94BbE25A05fC018`.
- [ ] Mode is `LiquidityLauncher / LBPStrategy`.
- [ ] LiquidityLauncher is `0x00004c4ccc709Ef590F7C81102C0689F0263D4e9`.
- [ ] LBPStrategy is `0xb98766A35cdc28415be0767D4EA41e39fBA3e000`.
- [ ] Initializer factory is `0x00cCa200BF124dBfA848937c553864f4B4CE0632`.
- [ ] FOLD says `discovered at deploy`.
- [ ] CCA auction says `discovered from LBPStrategy.InitializerCreated`.
- [ ] Auction FOLD is `120,000,000`.
- [ ] LP reserve is `30,000,000`.
- [ ] Total minted/distributed FOLD is `150,000,000`.
- [ ] Required raise is `400 ETH`.
- [ ] Floor price corresponds to `0.000012 ETH/FOLD`.
- [ ] Tick spacing corresponds to `1%` of floor price.
- [ ] CCA blocks line matches expected July 8 to July 10 window.
- [ ] FOLD timestamps line matches expected July 6 to July 10 lifecycle.
- [ ] FOLD `noMoreLocks` line is `1915884000` (`2030-09-17T14:00:00Z`).
- [ ] `claimBlock = migrationBlock`.
- [ ] Config hash is recorded in launch notes.
- [ ] Plan file is generated and inspected.

## 7. Deploy

Run:

```bash
pnpm sale --network mainnet --action deploy \
  --config ./deploy/sale/mainnet-sale.config.json \
  --propose-safe
```

Check:

- [ ] Deploy transaction mined.
- [ ] FOLD address printed and recorded.
- [ ] CCA auction address printed and recorded.
- [ ] Uniswap auction URL printed and opens.
- [ ] Deployment file is written.
- [ ] Safe proposal URL is printed.
- [ ] `*.safe-builder.json` is written.
- [ ] `*.safe-transactions.json` is written.
- [ ] Safe activation batch includes `FOLD.acceptOwnership()`.
- [ ] Safe activation batch includes `FOLD.setClaimSource(auction)`.
- [ ] If Predicate is enabled, Safe activation batch includes
      `PredicateValidationHook.setAuction(auction)`.
- [ ] If Safe API proposal fails, import `*.safe-builder.json` in Safe
      Transaction Builder and submit manually.
- [ ] If Safe proposal fails because operator is not a proposer, add operator as
      proposer and run `--action propose-safe` again.

## 8. Safe Activation

Safe owners approve and execute the activation batch.

Check in the Safe UI before execution:

- [ ] Safe address is `0x5429D8c7fD14023f3c414126F94BbE25A05fC018`.
- [ ] Batch has exactly 2 transactions if Predicate is disabled.
- [ ] Batch has exactly 3 transactions if Predicate is enabled.
- [ ] Transaction 1 target is FOLD and calldata is `acceptOwnership()`.
- [ ] Transaction 2 target is FOLD and calldata is `setClaimSource(auction)`.
- [ ] If Predicate is enabled, transaction 3 target is PredicateValidationHook
      and calldata is `setAuction(auction)`.
- [ ] Safe owners approve.
- [ ] Safe batch executes successfully.

## 9. Validate

Run:

```bash
pnpm sale --network mainnet --action validate \
  --config ./deploy/sale/mainnet-sale.config.json
```

Required pass conditions:

- [ ] `saleDeployer.protocolAdmin = Foundation Safe`.
- [ ] `FOLD.owner = Foundation Safe`.
- [ ] `FOLD.pendingOwner = zero`.
- [ ] `FOLD.CLAIM_SOURCE = auction`.
- [ ] `FOLD.BONDING_REGISTRY = BondingRegistry proxy`.
- [ ] Auction token is FOLD.
- [ ] Auction total supply is `120,000,000 FOLD`.
- [ ] Auction currency is ETH sentinel zero address.
- [ ] Auction tokens recipient is Foundation Safe.
- [ ] Auction funds recipient is LBPStrategy.
- [ ] Auction validation hook is correct.
- [ ] FOLD auction balance is `120,000,000 FOLD`.
- [ ] FOLD total supply is `150,000,000 FOLD`.
- [ ] LBP reserve is `30,000,000 FOLD`.
- [ ] LBP strategy values validate.
- [ ] LBP recipient is Foundation Safe.
- [ ] LBP position recipient is Foundation Safe.
- [ ] LiquidityLauncher is transfer-whitelisted on FOLD.
- [ ] LBPStrategy is transfer-whitelisted on FOLD.
- [ ] PositionManager is transfer-whitelisted on FOLD.
- [ ] Safe has `DEFAULT_ADMIN_ROLE`.
- [ ] Operator does not have `DEFAULT_ADMIN_ROLE`.
- [ ] Sale deployer does not have `DEFAULT_ADMIN_ROLE`.
- [ ] Safe has `MINTER_ROLE`.
- [ ] Safe has `WHITELIST_ROLE`.
- [ ] Safe has `LOCK_MANAGER_ROLE`.
- [ ] If Predicate is enabled, hook owner is Foundation Safe.
- [ ] If Predicate is enabled, hook auction is CCA auction.
- [ ] If Predicate is enabled, hook registry matches Predicate dashboard.
- [ ] If Predicate is enabled, hook policy ID matches Predicate dashboard.

## 10. Deployment Is Complete

Deployment is complete only when:

- [ ] `validate` passes.
- [ ] Uniswap auction URL opens on mainnet.
- [ ] Predicate verification path is confirmed on the Uniswap UI, if enabled.
- [ ] Foundation Safe owners have the deployment file, plan file, Safe proposal,
      FOLD address, auction address, and Uniswap URL.
- [ ] Team announcement uses the verified Uniswap mainnet auction URL.
