// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use anyhow::{Context, Result};
use e3_program_server::E3ProgramServer;
use e3_user_program::fhe_processor;

#[tokio::main]
async fn main() -> Result<()> {
    let bearer_token = std::env::var("INTERFOLD_PROGRAM_SERVER_TOKEN")
        .context("INTERFOLD_PROGRAM_SERVER_TOKEN must be set")?;
    let callback_origin = std::env::var("INTERFOLD_PROGRAM_SERVER_CALLBACK_ORIGIN")
        .context("INTERFOLD_PROGRAM_SERVER_CALLBACK_ORIGIN must be set")?;
    let server = E3ProgramServer::builder(|inputs| async move {
        Ok((
            vec![3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5],
            fhe_processor(&inputs),
        ))
    })
    .with_bearer_token(bearer_token)
    .with_callback_origin(callback_origin)
    .build()?;

    server.run().await?;
    Ok(())
}
