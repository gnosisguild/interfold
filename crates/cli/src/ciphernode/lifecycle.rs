// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use alloy::primitives::{Address, U256};
use anyhow::{bail, Result};
use e3_console::{log, Console};
use e3_utils::require_successful_receipt;

use super::context::{parse_address, ChainContext};
use super::utils::{format_amount, parse_amount};

pub(crate) async fn set_bond_owner(out: Console, ctx: &ChainContext, owner: &str) -> Result<()> {
    let owner = parse_address(owner)?;
    let receipt = ctx
        .bonding()
        .setBondOwner(owner)
        .send()
        .await?
        .get_receipt()
        .await?;
    require_successful_receipt("set bond owner", &receipt)?;
    log!(
        out,
        "Authorized bond owner {:#x} for operator {:#x} (tx: {:#x})",
        owner,
        ctx.operator(),
        receipt.transaction_hash
    );
    Ok(())
}

pub(crate) async fn propose_bond_owner(
    out: Console,
    ctx: &ChainContext,
    operator: &str,
    new_owner: &str,
) -> Result<()> {
    let operator = parse_address(operator)?;
    let new_owner = parse_address(new_owner)?;
    let receipt = ctx
        .bonding()
        .proposeBondOwner(operator, new_owner)
        .send()
        .await?
        .get_receipt()
        .await?;
    require_successful_receipt("propose bond owner", &receipt)?;
    log!(
        out,
        "Proposed {:#x} as owner of operator {:#x} (tx: {:#x})",
        new_owner,
        operator,
        receipt.transaction_hash
    );
    Ok(())
}

pub(crate) async fn accept_bond_owner(
    out: Console,
    ctx: &ChainContext,
    operator: &str,
) -> Result<()> {
    let operator = parse_address(operator)?;
    let receipt = ctx
        .bonding()
        .acceptBondOwner(operator)
        .send()
        .await?
        .get_receipt()
        .await?;
    require_successful_receipt("accept bond owner", &receipt)?;
    log!(
        out,
        "Accepted ownership of operator {:#x} (tx: {:#x})",
        operator,
        receipt.transaction_hash
    );
    Ok(())
}

pub(crate) async fn register(out: Console, ctx: &ChainContext, operator: Address) -> Result<()> {
    let receipt = ctx
        .bonding()
        .registerOperatorFor(operator)
        .send()
        .await?
        .get_receipt()
        .await?;
    require_successful_receipt("register ciphernode", &receipt)?;
    log!(
        out,
        "Registered operator {:#x} on {} (tx: {:#x})",
        operator,
        ctx.chain_label(),
        receipt.transaction_hash
    );
    Ok(())
}

pub(crate) async fn deregister(out: Console, ctx: &ChainContext, operator: Address) -> Result<()> {
    let receipt = ctx
        .bonding()
        .deregisterOperatorFor(operator)
        .send()
        .await?
        .get_receipt()
        .await?;
    require_successful_receipt("deregister ciphernode", &receipt)?;
    log!(
        out,
        "Deregistration requested for {:#x} (tx: {:#x})",
        operator,
        receipt.transaction_hash
    );
    Ok(())
}

pub(crate) async fn activate(out: Console, ctx: &ChainContext, operator: Address) -> Result<()> {
    register(out, ctx, operator).await
}

pub(crate) async fn deactivate(
    out: Console,
    ctx: &ChainContext,
    operator: Address,
    ticket_amount: Option<String>,
    license_amount: Option<String>,
) -> Result<()> {
    if ticket_amount.is_none() && license_amount.is_none() {
        bail!(
            "Provide --tickets and/or --license to specify what should be withdrawn for deactivation"
        );
    }

    if let Some(amount) = ticket_amount {
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
            "Removed {} tickets from {:#x} (tx: {:#x})",
            amount,
            operator,
            receipt.transaction_hash
        );
    }

    if let Some(amount) = license_amount {
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
            "Queued {} FOLD from {:#x} (tx: {:#x})",
            amount,
            operator,
            receipt.transaction_hash
        );
    }

    Ok(())
}

pub(crate) async fn status(out: Console, ctx: &ChainContext, operator: Address) -> Result<()> {
    let contract = ctx.bonding();
    let bond_owner = contract.bondOwnerOf(operator).call().await?;
    let pending_owner = contract.pendingBondOwnerOf(operator).call().await?;
    let ticket_balance: U256 = contract.getTicketBalance(operator).call().await?;
    let license_bond: U256 = contract.getLicenseBond(operator).call().await?;
    let available_tickets: U256 = contract.availableTickets(operator).call().await?;
    let is_registered: bool = contract.isRegistered(operator).call().await?;
    let is_active: bool = contract.isActive(operator).call().await?;
    let has_exit: bool = contract.hasExitInProgress(operator).call().await?;
    let pending = contract.pendingExits(operator).call().await?;
    let pending_tickets = pending.ticket;
    let pending_license = pending.license;
    let ticket_price: U256 = contract.ticketPrice().call().await?;
    let min_ticket_balance: U256 = contract.minTicketBalance().call().await?;
    let license_required: U256 = contract.licenseRequiredBond().call().await?;

    let ticket_token = ctx.ticket_token_address().await?;
    let license_token = ctx.license_token_address().await?;
    let ticket_decimals = ctx.erc20(ticket_token).decimals().call().await?;
    let license_decimals = ctx.erc20(license_token).decimals().call().await?;

    log!(out, "Ciphernode status on {}:", ctx.chain_label());
    log!(out, "  Operator key: {:#x}", operator);
    if bond_owner.is_zero() {
        log!(out, "  Bond owner: not configured");
    } else {
        log!(out, "  Bond owner: {:#x}", bond_owner);
    }
    if !pending_owner.is_zero() {
        log!(out, "  Pending bond owner: {:#x}", pending_owner);
    }
    log!(out, "  Registered: {}", is_registered);
    log!(out, "  Active: {}", is_active);
    log!(out, "  Exit pending: {}", has_exit);
    log!(
        out,
        "  Ticket balance: {} ({} available)",
        format_amount(ticket_balance, ticket_decimals),
        format_amount(available_tickets, ticket_decimals)
    );
    log!(
        out,
        "  License bond: {}",
        format_amount(license_bond, license_decimals)
    );
    log!(
        out,
        "  Pending exits: tickets={}, license={}",
        format_amount(pending_tickets, ticket_decimals),
        format_amount(pending_license, license_decimals)
    );
    log!(
        out,
        "  Requirements: minTickets={}, ticketPrice={} EKT, licenseBond={} FOLD",
        format_amount(min_ticket_balance, ticket_decimals),
        format_amount(ticket_price, ticket_decimals),
        format_amount(license_required, license_decimals)
    );
    Ok(())
}
