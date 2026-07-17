// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use super::*;

fn e3(chain_id: u64, id: &str) -> E3id {
    E3id::new(id, chain_id)
}

#[test]
fn add_and_remove_node() {
    let mut store = HashMap::new();
    NodeRegistry::add_node(&mut store, 1, "0xabc".into());
    assert!(store[&1].nodes.contains_key("0xabc"));

    NodeRegistry::remove_node(&mut store, 1, "0xabc");
    assert!(!store[&1].nodes.contains_key("0xabc"));

    // Removing an unknown node / chain is a no-op.
    NodeRegistry::remove_node(&mut store, 99, "0xdef");
}

#[test]
fn available_tickets_accounts_for_price_and_jobs() {
    let mut store = HashMap::new();
    NodeRegistry::set_ticket_price(&mut store, 1, U256::from(10));
    NodeRegistry::set_ticket_balance(&mut store, 1, "0xabc".into(), U256::from(55));
    NodeRegistry::set_operator_active(&mut store, 1, "0xabc".into(), true);

    // floor(55 / 10) = 5 tickets, no active jobs yet.
    assert_eq!(store[&1].available_tickets("0xabc"), 5);

    NodeRegistry::record_committee_published(&mut store, &e3(1, "7"), &["0xabc".into()]);
    // One active job now -> 4 available.
    assert_eq!(store[&1].available_tickets("0xabc"), 4);
    assert_eq!(store[&1].nodes["0xabc"].active_jobs, 1);
}

#[test]
fn zero_price_yields_no_tickets() {
    let mut store = HashMap::new();
    NodeRegistry::set_ticket_balance(&mut store, 1, "0xabc".into(), U256::from(100));
    assert_eq!(store[&1].available_tickets("0xabc"), 0);
}

#[test]
fn operator_active_applies_only_to_the_event_chain() {
    let mut store = HashMap::new();
    NodeRegistry::add_node(&mut store, 1, "0xabc".into());
    NodeRegistry::add_node(&mut store, 2, "0xabc".into());
    NodeRegistry::set_operator_active(&mut store, 1, "0xabc".into(), true);
    assert!(store[&1].nodes["0xabc"].active);
    assert!(!store[&2].nodes["0xabc"].active);
}

#[test]
fn eligibility_update_invalidates_only_the_target_chain() {
    let mut store = HashMap::new();
    NodeRegistry::add_node(&mut store, 1, "0xabc".into());
    NodeRegistry::add_node(&mut store, 2, "0xabc".into());
    NodeRegistry::set_operator_active(&mut store, 1, "0xabc".into(), true);
    NodeRegistry::set_operator_active(&mut store, 2, "0xabc".into(), true);

    NodeRegistry::invalidate_operator_activity(&mut store, 1);

    assert!(!store[&1].nodes["0xabc"].active);
    assert!(store[&2].nodes["0xabc"].active);
}

#[test]
fn release_committee_jobs_is_idempotent() {
    let mut store = HashMap::new();
    let id = e3(1, "7");
    NodeRegistry::record_committee_published(&mut store, &id, &["0xabc".into(), "0xdef".into()]);
    assert_eq!(store[&1].nodes["0xabc"].active_jobs, 1);
    assert_eq!(store[&1].nodes["0xdef"].active_jobs, 1);

    NodeRegistry::release_committee_jobs(&mut store, &id, "test");
    assert_eq!(store[&1].nodes["0xabc"].active_jobs, 0);
    assert_eq!(store[&1].nodes["0xdef"].active_jobs, 0);
    assert!(!store[&1].e3_committees.contains_key(&committee_key(&id)));

    // Second release does not underflow.
    NodeRegistry::release_committee_jobs(&mut store, &id, "test-again");
    assert_eq!(store[&1].nodes["0xabc"].active_jobs, 0);
}

#[test]
fn get_nodes_with_tickets_filters_inactive_and_empty() {
    let mut store = HashMap::new();
    NodeRegistry::set_ticket_price(&mut store, 1, U256::from(10));
    NodeRegistry::set_ticket_balance(&mut store, 1, "active".into(), U256::from(30));
    NodeRegistry::set_operator_active(&mut store, 1, "active".into(), true);
    // Inactive node with balance is excluded.
    NodeRegistry::set_ticket_balance(&mut store, 1, "inactive".into(), U256::from(30));

    let with_tickets = store[&1].get_nodes_with_tickets();
    assert_eq!(with_tickets.len(), 1);
    assert_eq!(with_tickets[0].0, "active");
    assert_eq!(with_tickets[0].1, 3);
}

#[test]
fn open_committees_lists_only_unreleased() {
    let mut store = HashMap::new();
    let a = e3(1, "1");
    let b = e3(1, "2");
    NodeRegistry::record_committee_published(&mut store, &a, &["0xabc".into()]);
    NodeRegistry::record_committee_published(&mut store, &b, &["0xabc".into(), "0xdef".into()]);

    let open = NodeRegistry::open_committees(&store);
    assert_eq!(open.len(), 2);

    // Releasing one removes it from the open set.
    NodeRegistry::release_committee_jobs(&mut store, &a, "test");
    let open = NodeRegistry::open_committees(&store);
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].committee_key, committee_key(&b));
    assert_eq!(open[0].chain_id, 1);
    assert_eq!(
        open[0].members,
        vec!["0xabc".to_string(), "0xdef".to_string()]
    );

    // Fully drained -> empty.
    NodeRegistry::release_committee_jobs(&mut store, &b, "test");
    assert!(NodeRegistry::open_committees(&store).is_empty());
}
