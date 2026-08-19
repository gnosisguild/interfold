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
            let symbol = metadata.symbol().call().await?;
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
            // `--amount` is collateral, not a ticket count: the contract holds a
            // collateral balance and derives tickets as floor(balance / ticketPrice).
            // Read the count back so a remainder (105 USDC at 10 USDC/ticket) is not
            // reported as a ticket the operator does not have. The deposit already
            // succeeded, so a failed read must not fail the command.
            match ctx.bonding().availableTickets(operator).call().await {
                Ok(tickets) => log!(
                    out,
                    "Deposited {} {} for operator {:#x}. Ticket balance is now {} tickets (tx: {:#x})",
                    amount,
                    symbol,
                    operator,
                    tickets,
                    receipt.transaction_hash
                ),
                Err(err) => log!(
                    out,
                    "Deposited {} {} for operator {:#x} (tx: {:#x}). Could not read the ticket balance: {err}",
                    amount,
                    symbol,
                    operator,
                    receipt.transaction_hash
                ),
            }
        }
        TicketCommands::Burn { amount } => {
            let ticket_contract = ctx.ticket_token_address().await?;
            let ticket_metadata = ctx.erc20(ticket_contract);
            let decimals = ticket_metadata.decimals().call().await?;
            let symbol = ticket_metadata.symbol().call().await?;
            let parsed = parse_amount(&amount, decimals)?;
            let receipt = ctx
                .bonding()
                .removeTicketBalanceFor(operator, parsed)
                .send()
                .await?
                .get_receipt()
                .await?;
            require_successful_receipt("remove ticket balance", &receipt)?;
            // The burn already succeeded, so a failed read must not fail the command.
            match ctx.bonding().availableTickets(operator).call().await {
                Ok(tickets) => log!(
                    out,
                    "Burned {} {} from operator {:#x}. Ticket balance is now {} tickets (tx: {:#x})",
                    amount,
                    symbol,
                    operator,
                    tickets,
                    receipt.transaction_hash
                ),
                Err(err) => log!(
                    out,
                    "Burned {} {} from operator {:#x} (tx: {:#x}). Could not read the ticket balance: {err}",
                    amount,
                    symbol,
                    operator,
                    receipt.transaction_hash
                ),
            }
        }
    }

    Ok(())
}
