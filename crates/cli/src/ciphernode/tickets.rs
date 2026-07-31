// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use alloy::primitives::Address;
use anyhow::Result;
use e3_console::{log, Console};
use e3_utils::require_successful_receipt;

use super::context::ChainContext;
use super::utils::{ensure_allowance, parse_amount};
use super::TicketCommands;

pub(crate) async fn execute(
    out: Console,
    ctx: &ChainContext,
    operator: Address,
    command: TicketCommands,
) -> Result<()> {
    match command {
        TicketCommands::Buy { amount } => {
            let ticket_contract = ctx.ticket_token_address().await?;
            let underlying = ctx.ticket_underlying_address().await?;
            let metadata = ctx.erc20(underlying);
            let decimals = metadata.decimals().call().await?;
            let parsed = parse_amount(&amount, decimals)?;
            ensure_allowance(ctx, underlying, ticket_contract, parsed).await?;
            let receipt = ctx
                .bonding()
                .addTicketBalanceFor(operator, parsed)
                .send()
                .await?
                .get_receipt()
                .await?;
            require_successful_receipt("add ticket balance", &receipt)?;
            log!(
                out,
                "Purchased {} tickets for operator {:#x} (tx: {:#x})",
                amount,
                operator,
                receipt.transaction_hash
            );
        }
        TicketCommands::Burn { amount } => {
            let ticket_contract = ctx.ticket_token_address().await?;
            let decimals = ctx.erc20(ticket_contract).decimals().call().await?;
            let parsed = parse_amount(&amount, decimals)?;
            let receipt = ctx
                .bonding()
                .removeTicketBalanceFor(operator, parsed)
                .send()
                .await?
                .get_receipt()
                .await?;
            require_successful_receipt("remove ticket balance", &receipt)?;
            log!(
                out,
                "Removed {} tickets from operator {:#x} (tx: {:#x})",
                amount,
                operator,
                receipt.transaction_hash
            );
        }
    }

    Ok(())
}
