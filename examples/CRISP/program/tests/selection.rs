// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! CRISP's selection rule: the most recent entry per slot whose bytes reproduce its commitment.
//!
//! The rule lives here rather than in `e3-compute-provider` because it is CRISP's answer to CRISP's
//! problem — an append-only tree that anyone may write to. A program where every input counts wants
//! the crate's default instead.

use e3_compute_provider::policy::PublishedInput;
use e3_user_program::policy::latest_usable_per_slot;

fn slot(tag: u8) -> [u8; 20] {
    let mut address = [0u8; 20];
    address[19] = tag;
    address
}

/// Builds one slot's entry sequence. `true` is an honest entry; `false` an entry whose bytes do not
/// reproduce its commitment, which is what a third party can publish.
fn entries(pattern: &[bool], slots: &[[u8; 20]]) -> Vec<usize> {
    let stored: Vec<[u8; 32]> = (0..pattern.len()).map(|i| [i as u8; 32]).collect();
    let bytes = vec![0u8];

    let inputs: Vec<PublishedInput> = pattern
        .iter()
        .enumerate()
        .map(|(index, honest)| PublishedInput {
            index,
            ciphertext: &bytes,
            commitment: Some(&stored[index]),
            metadata: &slots[index],
            // An honest entry's bytes reproduce the stored commitment; a poisoned one's do not.
            recomputed: Some(if *honest { stored[index] } else { [0xff; 32] }),
        })
        .collect();

    latest_usable_per_slot(&inputs)
}

/// Every sequence for one slot resolves to that slot's most recent honest entry.
///
/// The case that matters most is honest → poisoned → honest: selection has to move *forward* to the
/// later honest entry, not fall back to the first. Get that wrong and one poison would make every
/// later re-vote by that voter invisible.
#[test]
fn selection_picks_the_latest_honest_entry_in_every_sequence() {
    let cases: &[(&[bool], Vec<usize>, &str)] = &[
        (&[true], vec![0], "a single honest entry"),
        (
            &[false],
            vec![],
            "a single poisoned entry contributes nothing",
        ),
        (&[true, false], vec![0], "poisoned append falls back"),
        (
            &[false, true],
            vec![1],
            "an honest entry after a poisoned one wins",
        ),
        (&[true, true], vec![1], "an honest re-vote replaces"),
        (
            &[true, false, true],
            vec![2],
            "honest, poisoned, honest picks the last honest",
        ),
        (
            &[true, true, false],
            vec![1],
            "poisoned at the end falls back one step",
        ),
        (
            &[true, false, false],
            vec![0],
            "two poisoned appends still fall back to the first",
        ),
        (
            &[false, false, true],
            vec![2],
            "honest after two poisoned wins",
        ),
        (
            &[false, true, false],
            vec![1],
            "honest in the middle survives a later poison",
        ),
        (
            &[true, false, true, false],
            vec![2],
            "falls back past a trailing poison",
        ),
        (
            &[false, false, false],
            vec![],
            "an all-poisoned slot contributes nothing",
        ),
    ];

    let one_slot: Vec<[u8; 20]> = (0..8).map(|_| slot(1)).collect();
    for (pattern, expected, description) in cases {
        assert_eq!(
            &entries(pattern, &one_slot[..pattern.len()]),
            expected,
            "{description}"
        );
    }
}

/// Sequences interleaved across slots must not bleed into one another.
#[test]
fn interleaved_slots_resolve_independently() {
    // A-honest, B-poisoned, A-poisoned, B-honest.
    let pattern = [true, false, false, true];
    let slots = [slot(1), slot(2), slot(1), slot(2)];

    assert_eq!(
        entries(&pattern, &slots),
        vec![0, 3],
        "each slot resolves on its own entries, in tree order"
    );
}

/// An entry whose metadata is not a slot address cannot be grouped, so it is not selected. It still
/// contributes a leaf — that is the crate's guarantee, not the policy's.
#[test]
fn an_entry_without_a_valid_slot_is_not_selected() {
    let stored = [7u8; 32];
    let bytes = vec![0u8];
    let malformed = [0u8; 4];

    let inputs = vec![PublishedInput {
        index: 0,
        ciphertext: &bytes,
        commitment: Some(&stored),
        metadata: &malformed,
        recomputed: Some(stored),
    }];

    assert!(latest_usable_per_slot(&inputs).is_empty());
}
