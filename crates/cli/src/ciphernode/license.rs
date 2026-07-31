// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use alloy::primitives::{Address, U256};
use anyhow::Result;
use e3_console::{log, Console};
use e3_utils::require_successful_receipt;

use super::context::ChainContext;
use super::utils::{ensure_allowance, parse_amount};
use super::LicenseCommands;

pub(crate) async fn execute(
    out: Console,
    ctx: &ChainContext,
    operator: Address,
    command: LicenseCommands,
) -> Result<()> {
    match command {
        LicenseCommands::Bond { amount } => {
            bond_license(out, ctx, operator, &amount).await?;
        }
        LicenseCommands::Unbond { amount } => {
            let license = ctx.license_token_address().await?;
            let decimals = ctx.erc20(license).decimals().call().await?;
            let parsed = parse_amount(&amount, decimals)?;
            let receipt = ctx
                .bonding()
                .unbondLicenseFor(operator, parsed)
                .send()
                .await?
                .get_receipt()
                .await?;
            require_successful_receipt("unbond license", &receipt)?;
            log!(
                out,
                "Queued {} FOLD for operator {:#x} (tx: {:#x})",
                amount,
                operator,
                receipt.transaction_hash
            );
        }
        LicenseCommands::Claim {
            max_ticket,
            max_license,
        } => {
            let ticket_decimals = ctx
                .erc20(ctx.ticket_token_address().await?)
                .decimals()
                .call()
                .await?;
            let license_decimals = ctx
                .erc20(ctx.license_token_address().await?)
                .decimals()
                .call()
                .await?;

            let ticket = if let Some(value) = max_ticket {
                parse_amount(&value, ticket_decimals)?
            } else {
                U256::MAX
            };
            let license = if let Some(value) = max_license {
                parse_amount(&value, license_decimals)?
            } else {
                U256::MAX
            };
            let receipt = ctx
                .bonding()
                .claimExitsFor(operator, ticket, license)
                .send()
                .await?
                .get_receipt()
                .await?;
            require_successful_receipt("claim exits", &receipt)?;
            log!(
                out,
                "Claimed exits for operator {:#x} (tx: {:#x})",
                operator,
                receipt.transaction_hash
            );
        }
    }

    Ok(())
}

async fn bond_license(
    out: Console,
    ctx: &ChainContext,
    operator: Address,
    amount: &str,
) -> Result<()> {
    let license = ctx.license_token_address().await?;
    let erc20 = ctx.erc20(license);
    let decimals = erc20.decimals().call().await?;
    let parsed = parse_amount(amount, decimals)?;
    ensure_allowance(ctx, license, ctx.bonding_registry(), parsed).await?;
    let receipt = ctx
        .bonding()
        .bondLicenseFor(operator, parsed)
        .send()
        .await?
        .get_receipt()
        .await?;
    require_successful_receipt("bond license", &receipt)?;
    log!(
        out,
        "Bonded {} FOLD for operator {:#x} (tx: {:#x})",
        amount,
        operator,
        receipt.transaction_hash
    );
    Ok(())
}
