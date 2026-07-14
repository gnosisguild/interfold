// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use anyhow::*;
use clap::Subcommand;
use e3_config::AppConfig;
use e3_console::Console;
use zeroize::Zeroizing;

use crate::{
    helpers::{ensure_hex_zeroizing, read_secret_line},
    wallet_get, wallet_set,
};

#[derive(Subcommand, Clone, Debug)]
pub enum WalletCommands {
    /// Set wallet private key
    Set {
        /// The private key - note we are leaving as hex string as it is easier to manage with
        /// the allow Signer coercion
        #[arg(
            long = "private-key",
            value_parser = ensure_hex_zeroizing,
            conflicts_with = "private_key_stdin"
        )]
        private_key: Option<Zeroizing<String>>,

        /// Read the private key from one line on stdin
        #[arg(long, conflicts_with = "private_key")]
        private_key_stdin: bool,
    },
    /// Get your wallet address
    Get,
}

pub async fn execute(out: Console, command: WalletCommands, config: AppConfig) -> Result<()> {
    match command {
        WalletCommands::Set {
            private_key,
            private_key_stdin,
        } => {
            let private_key = if private_key_stdin {
                Some(read_secret_line(
                    &mut std::io::stdin().lock(),
                    "private key",
                )?)
            } else {
                private_key
            };
            wallet_set::execute(out, &config, private_key).await?
        }
        WalletCommands::Get => wallet_get::execute(out, &config).await?,
    };

    Ok(())
}
