// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use anyhow::Result;
use e3_console::{log, Console};
use e3_utils::require_successful_receipt;

use super::context::ChainContext;
use super::utils::{ensure_allowance, parse_amount};
use super::TicketCommands;

pub(crate) async fn execute(
    out: Console,
    ctx: &ChainContext,
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
                .addTicketBalance(parsed)
                .send()
                .await?
                .get_receipt()
                .await?;
            require_successful_receipt("add ticket balance", &receipt)?;
            log!(
                out,
                "Purchased {} tickets (tx: {:#x})",
                amount,
                receipt.transaction_hash
            );
        }
        TicketCommands::Burn { amount } => {
            let ticket_contract = ctx.ticket_token_address().await?;
            let decimals = ctx.erc20(ticket_contract).decimals().call().await?;
            let parsed = parse_amount(&amount, decimals)?;
            let receipt = ctx
                .bonding()
                .removeTicketBalance(parsed)
                .send()
                .await?
                .get_receipt()
                .await?;
            require_successful_receipt("remove ticket balance", &receipt)?;
            log!(
                out,
                "Removed {} tickets (tx: {:#x})",
                amount,
                receipt.transaction_hash
            );
        }
    }

    Ok(())
}
