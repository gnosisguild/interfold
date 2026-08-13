// SPDX-License-Identifier: LGPL-4.0-only

//! E3 ticket sortition and selector dispatch.

use super::*;

impl Handler<TypedEvent<E3Requested>> for Sortition {
    type Result = ();
    fn handle(&mut self, msg: TypedEvent<E3Requested>, _: &mut Self::Context) -> Self::Result {
        let e3_id = msg.e3_id.clone();
        if !self.sortition_seeds.contains_key(&e3_id) {
            info!(e3_id = %e3_id, "Waiting for the delayed sortition seed");
            self.pending_requests.insert(e3_id, msg);
            return;
        }
        self.perform_sortition(msg);
    }
}

impl Sortition {
    pub(super) fn perform_sortition(&mut self, msg: TypedEvent<E3Requested>) {
        let e3_id = msg.e3_id.clone();
        let chain_id = msg.e3_id.chain_id();
        let seed = self.sortition_seeds[&e3_id];
        let threshold_m = msg.threshold_m;
        let threshold_n = msg.threshold_n;
        let buffer = ticket_sortition::calculate_buffer_size(threshold_m, threshold_n);
        let total_selection_size = threshold_n + buffer;
        let snapshot = self.node_state.get().and_then(|state| {
            state
                .get(&chain_id)
                .and_then(|state| state.sortition_snapshot(&e3_id))
        });

        info!(
            e3_id = %e3_id,
            threshold_m = threshold_m,
            threshold_n = threshold_n,
            buffer = buffer,
            total_selection_size = total_selection_size,
            "Performing Sortition with buffer"
        );

        let node_index = match snapshot {
            Some(snapshot)
                if snapshot.request_block == msg.request_block
                    && !snapshot.ticket_price.is_zero() =>
            {
                self.get_node_index(
                    e3_id.clone(),
                    seed,
                    total_selection_size,
                    chain_id,
                    snapshot,
                )
            }
            Some(snapshot) if snapshot.request_block != msg.request_block => {
                self.bus.err(
                    EType::Sortition,
                    anyhow!(
                        "E3 {} has inconsistent sortition context: request block {} != {}",
                        e3_id,
                        msg.request_block,
                        snapshot.request_block
                    ),
                );
                None
            }
            Some(_) => {
                self.bus.err(
                    EType::Sortition,
                    anyhow!("E3 {} has a zero request-time ticket price", e3_id),
                );
                None
            }
            None => {
                self.bus.err(
                    EType::Sortition,
                    anyhow!("E3 {} has no request-time sortition snapshot", e3_id),
                );
                None
            }
        };

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
