// SPDX-License-Identifier: LGPL-3.0-only

//! Terminal publication and result handling.

mod publication;
mod results;

use super::*;

impl PublicKeyAggregator {
    pub fn handle_member_expelled(
        &mut self,
        node: &str,
        ec: &EventContext<Sequenced>,
    ) -> Result<()> {
        self.state.try_mutate(ec, |state| {
            PublicKeyAggregation::handle_member_expelled(state, node)
        })
    }
}
