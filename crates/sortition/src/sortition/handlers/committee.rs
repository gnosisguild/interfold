// SPDX-License-Identifier: LGPL-4.0-only

//! Persist finalized committees and enrich expulsion facts.

use super::*;

impl Handler<TypedEvent<CommitteeFinalized>> for Sortition {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<CommitteeFinalized>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let (mut msg, ec) = msg.into_components();
        msg.sort_by_score();
        trap(EType::Sortition, &self.bus.with_ec(&ec), || {
            info!(
                e3_id = %msg.e3_id,
                committee_size = msg.committee.len(),
                "Storing finalized committee"
            );

            self.finalized_committees
                .try_mutate(&ec, |mut committees| {
                    committees.insert(msg.e3_id.clone(), Committee::new(msg.committee.clone()));
                    Ok(committees)
                })?;

            // Drain any expulsions that arrived before the committee was finalized (C18).
            if let Some(buffered) = self.pending_expulsions.remove(&msg.e3_id) {
                info!(
                    e3_id = %msg.e3_id,
                    count = buffered.len(),
                    "Sortition: draining buffered pre-finalization expulsion(s)"
                );
                for (data, buffered_ec) in buffered {
                    if let Err(e) = self.try_resolve_and_publish_expulsion(data, buffered_ec) {
                        warn!(
                            e3_id = %msg.e3_id,
                            error = %e,
                            "Sortition: failed to process buffered expulsion after finalization"
                        );
                    }
                }
            }

            Ok(())
        })
    }
}

impl Handler<TypedEvent<CommitteeMemberExpelled>> for Sortition {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<CommitteeMemberExpelled>,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let (data, ec) = msg.into_components();

        // Only process raw events from chain (party_id not yet resolved).
        // Events we re-publish with party_id set will also arrive here; ignore them.
        if data.party_id.is_some() {
            return;
        }

        trap(EType::Sortition, &self.bus.with_ec(&ec), || {
            if self.try_resolve_and_publish_expulsion(data.clone(), ec.clone())? {
                return Ok(());
            }

            // Committee not finalized yet — buffer until CommitteeFinalized arrives (C18) instead
            // of dropping the expulsion, which would otherwise leave a known-bad member in the
            // committee until the round times out.
            warn!(
                node = %data.node,
                e3_id = %data.e3_id,
                "CommitteeMemberExpelled arrived before committee finalized; buffering until finalization"
            );
            self.pending_expulsions
                .entry(data.e3_id.clone())
                .or_default()
                .push((data, ec));
            Ok(())
        })
    }
}
