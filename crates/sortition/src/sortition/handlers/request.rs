// SPDX-License-Identifier: LGPL-4.0-only

//! E3 ticket sortition and selector dispatch.

use super::*;

impl Handler<TypedEvent<E3Requested>> for Sortition {
    type Result = ();
    fn handle(&mut self, msg: TypedEvent<E3Requested>, _: &mut Self::Context) -> Self::Result {
        let e3_id = msg.e3_id.clone();
        let chain_id = msg.e3_id.chain_id();
        let seed = msg.seed;
        let threshold_m = msg.threshold_m;
        let threshold_n = msg.threshold_n;
        let buffer = ticket_sortition::calculate_buffer_size(threshold_m, threshold_n);
        let total_selection_size = threshold_n + buffer;

        info!(
            e3_id = %e3_id,
            threshold_m = threshold_m,
            threshold_n = threshold_n,
            buffer = buffer,
            total_selection_size = total_selection_size,
            "Performing Sortition with buffer"
        );

        let node_index = self.get_node_index(e3_id.clone(), seed, total_selection_size, chain_id);

        match &node_index {
            Some((index, ticket_id)) => {
                info!(
                    e3_id = %e3_id,
                    node = %self.address,
                    index = index,
                    ticket_id = ?ticket_id,
                    "This node was SELECTED for sortition"
                );
            }
            None => {
                info!(
                    e3_id = %e3_id,
                    node = %self.address,
                    "This node was NOT selected for sortition"
                );
            }
        }

        self.ciphernode_selector
            .do_send(WithSortitionTicket::new(msg, node_index, &self.address))
    }
}
