// SPDX-License-Identifier: LGPL-4.0-only

//! Apply node-registry, collateral, activation, and configuration facts.

use super::*;

impl Handler<TypedEvent<CiphernodeAdded>> for Sortition {
    type Result = ();

    fn handle(&mut self, msg: TypedEvent<CiphernodeAdded>, _: &mut Self::Context) -> Self::Result {
        let (msg, ec) = msg.into_components();
        trap(EType::Sortition, &self.bus.with_ec(&ec), || {
            let chain_id = msg.chain_id;
            let addr = msg.address.clone();

            self.node_state.try_mutate(&ec, |mut state_map| {
                NodeRegistry::add_node(&mut state_map, chain_id, addr.clone());
                Ok(state_map)
            })?;
            self.backends.try_mutate(&ec, move |mut list_map| {
                let default_backend = list_map
                    .get(&u64::MAX)
                    .cloned()
                    .unwrap_or_else(SortitionBackend::score);

                list_map
                    .entry(chain_id)
                    .or_insert_with(|| default_backend)
                    .add(addr);
                Ok(list_map)
            })?;
            Ok(())
        })
    }
}

impl Handler<TypedEvent<CiphernodeRemoved>> for Sortition {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<CiphernodeRemoved>,
        _: &mut Self::Context,
    ) -> Self::Result {
        let (msg, ec) = msg.into_components();
        trap(EType::Sortition, &self.bus.with_ec(&ec), || {
            let chain_id = msg.chain_id;
            let addr = msg.address.clone();

            self.node_state.try_mutate(&ec, |mut state_map| {
                NodeRegistry::remove_node(&mut state_map, chain_id, &addr);
                Ok(state_map)
            })?;
            self.backends.try_mutate(&ec, move |mut list_map| {
                if let Some(backend) = list_map.get_mut(&chain_id) {
                    backend.remove(addr);
                }
                Ok(list_map)
            })?;
            Ok(())
        })
    }
}

impl Handler<TypedEvent<TicketBalanceUpdated>> for Sortition {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<TicketBalanceUpdated>,
        _: &mut Self::Context,
    ) -> Self::Result {
        let (msg, ec) = msg.into_components();
        trap(EType::Sortition, &self.bus.with_ec(&ec), || {
            self.node_state.try_mutate(&ec, |mut state_map| {
                NodeRegistry::set_ticket_balance(
                    &mut state_map,
                    msg.chain_id,
                    msg.operator.clone(),
                    msg.new_balance,
                );
                Ok(state_map)
            })
        })
    }
}

impl Handler<TypedEvent<OperatorActivationChanged>> for Sortition {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<OperatorActivationChanged>,
        _: &mut Self::Context,
    ) -> Self::Result {
        let (msg, ec) = msg.into_components();
        trap(EType::Sortition, &self.bus.with_ec(&ec), || {
            self.node_state.try_mutate(&ec, |mut state_map| {
                NodeRegistry::set_operator_active(&mut state_map, msg.operator.clone(), msg.active);
                Ok(state_map)
            })
        })
    }
}

impl Handler<TypedEvent<ConfigurationUpdated>> for Sortition {
    type Result = ();

    fn handle(
        &mut self,
        msg: TypedEvent<ConfigurationUpdated>,
        _: &mut Self::Context,
    ) -> Self::Result {
        let (msg, ec) = msg.into_components();
        trap(EType::Sortition, &self.bus.with_ec(&ec), || {
            if msg.parameter == "ticketPrice" {
                self.node_state.try_mutate(&ec, |mut state_map| {
                    NodeRegistry::set_ticket_price(&mut state_map, msg.chain_id, msg.new_value);
                    Ok(state_map)
                })?;
            }
            Ok(())
        })
    }
}
