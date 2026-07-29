// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use anyhow::{bail, Result};
use clap::{Args, Subcommand};
use e3_config::AppConfig;

mod context;
mod lifecycle;
pub mod setup;
mod utils;

use context::ChainContext;
use e3_console::Console;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::helpers::{ensure_hex_zeroizing, parse_zeroizing};

#[derive(Debug, Args, Clone, Default, Serialize, Deserialize)]
pub struct ChainArgs {
    /// Chain name as defined in the interfold config (defaults to the first entry)
    #[arg(long = "chain")]
    pub chain: Option<String>,
}

impl ChainArgs {
    fn selection(&self) -> Option<&str> {
        self.chain.as_deref()
    }
}

#[derive(Subcommand, Clone, Debug)]
pub enum CiphernodeCommands {
    /// Setup local ciphernode configuration
    Setup {
        /// An rpc url for interfold to connect to
        #[arg(long = "rpc-url", short = 'r')]
        rpc_url: Option<String>,

        /// The password
        #[arg(
            short = 'p',
            long,
            value_parser = parse_zeroizing,
            conflicts_with = "password_stdin"
        )]
        password: Option<Zeroizing<String>>,

        /// Read the password from the first requested line on stdin
        #[arg(long, conflicts_with = "password")]
        password_stdin: bool,

        /// Wallet Private Key
        #[arg(
            short = 'k',
            long,
            value_parser = ensure_hex_zeroizing,
            conflicts_with = "private_key_stdin"
        )]
        private_key: Option<Zeroizing<String>>,

        /// Read the private key from the next requested line on stdin
        #[arg(long, conflicts_with = "private_key")]
        private_key_stdin: bool,
    },
    /// Irreversibly authorize the wallet that will own this node's collateral
    SetBondOwner {
        /// Cold wallet or Safe that will fund and control the bond
        #[arg(long = "owner", value_name = "ADDRESS")]
        owner: String,
        #[command(flatten)]
        chain: ChainArgs,
    },
    /// Display the current on-chain status for this operator
    Status {
        #[command(flatten)]
        chain: ChainArgs,
    },
}

pub async fn execute(out: Console, command: CiphernodeCommands, config: &AppConfig) -> Result<()> {
    match command {
        CiphernodeCommands::SetBondOwner { chain, owner } => {
            let ctx = ChainContext::new(config, chain.selection()).await?;
            lifecycle::set_bond_owner(out, &ctx, &owner).await?
        }
        CiphernodeCommands::Status { chain } => {
            let ctx = ChainContext::new(config, chain.selection()).await?;
            lifecycle::status(out, &ctx).await?
        }
        CiphernodeCommands::Setup { .. } => {
            bail!(
                "Cannot run `interfold ciphernode setup` when a configuration already exists: {:?}",
                config.config_file()
            );
        }
    }

    Ok(())
}
