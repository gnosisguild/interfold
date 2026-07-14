// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use anyhow::Result;
use e3_program_server::E3ProgramServer;
use e3_user_program::fhe_processor;

#[tokio::main]
async fn main() -> Result<()> {
    let server = E3ProgramServer::builder(|inputs| async move {
        Ok((
            vec![3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5],
            fhe_processor(&inputs),
        ))
    })
    .build()?;

    server.run().await?;
    Ok(())
}
