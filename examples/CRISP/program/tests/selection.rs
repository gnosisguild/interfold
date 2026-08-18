// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! CRISP's selection rule: the end of each slot's chain of usable entries.
//!
//! The rule lives here rather than in `e3-compute-provider` because it is CRISP's answer to CRISP's
//! problem — an append-only tree that anyone may write to. A program where every input counts wants
//! the crate's default instead.

use e3_compute_provider::policy::PublishedInput;
use e3_user_program::policy::chain_head_per_slot;

/// One published entry, as a test states it.
#[derive(Clone, Copy)]
struct Entry {
    slot: u8,
    /// Whether the bytes reproduce the commitment. A third party can publish an entry where they
    /// do not, and nobody can tell until here.
    usable: bool,
    /// The tree index this entry names as the one it extends, or `None` for nothing.
    parent: Option<usize>,
}

/// An honest entry extending `parent`.
fn good(slot: u8, parent: Option<usize>) -> Entry {
    Entry {
        slot,
        usable: true,
        parent,
    }
}

/// An entry whose published bytes do not reproduce its commitment.
fn poisoned(slot: u8, parent: Option<usize>) -> Entry {
    Entry {
        slot,
        usable: false,
        parent,
    }
}

fn slot_address(tag: u8) -> [u8; 20] {
    let mut address = [0u8; 20];
    address[19] = tag;
    address
}

/// `abi.encodePacked(address, uint40)`, which is what `CRISPProgram` publishes per input.
fn metadata(entry: &Entry) -> Vec<u8> {
    let parent_plus_one = entry.parent.map_or(0u64, |index| index as u64 + 1);

    let mut bytes = slot_address(entry.slot).to_vec();
    bytes.extend_from_slice(&parent_plus_one.to_be_bytes()[3..]);
    bytes
}

fn select(entries: &[Entry]) -> Vec<usize> {
    let stored: Vec<[u8; 32]> = (0..entries.len()).map(|i| [i as u8; 32]).collect();
    let metadatas: Vec<Vec<u8>> = entries.iter().map(metadata).collect();
    let bytes = vec![0u8];

    let inputs: Vec<PublishedInput> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| PublishedInput {
            index,
            ciphertext: &bytes,
            commitment: Some(&stored[index]),
            metadata: &metadatas[index],
            recomputed: Some(if entry.usable {
                stored[index]
            } else {
                [0xff; 32]
            }),
        })
        .collect();

    chain_head_per_slot(&inputs)
}

/// A chain of honest entries resolves to its end.
#[test]
fn selection_follows_the_chain_to_its_end() {
    assert_eq!(select(&[good(1, None)]), vec![0], "a single entry");
    assert_eq!(
        select(&[good(1, None), good(1, Some(0))]),
        vec![1],
        "an entry extending the first replaces it"
    );
    assert_eq!(
        select(&[good(1, None), good(1, Some(0)), good(1, Some(1))]),
        vec![2],
        "a three-entry chain resolves to its end"
    );
}

/// A poisoned entry does not become the head, and the next honest input extends the same parent it
/// did. This is the whole point of naming the parent: a slot cannot be frozen against masking, and
/// a slot that cannot be masked is one where every later input is provably its owner voting again.
#[test]
fn a_poisoned_entry_does_not_freeze_the_slot() {
    assert_eq!(
        select(&[poisoned(1, None)]),
        Vec::<usize>::new(),
        "a single poisoned entry contributes nothing"
    );
    assert_eq!(
        select(&[good(1, None), poisoned(1, Some(0))]),
        vec![0],
        "a poisoned append leaves the head where it was"
    );
    assert_eq!(
        select(&[good(1, None), poisoned(1, Some(0)), good(1, Some(0))]),
        vec![2],
        "the next honest entry names the same parent and takes the head"
    );
    assert_eq!(
        select(&[
            good(1, None),
            poisoned(1, Some(0)),
            poisoned(1, Some(0)),
            good(1, Some(0)),
        ]),
        vec![3],
        "repeated poisoning does not exhaust the slot"
    );
    assert_eq!(
        select(&[poisoned(1, None), good(1, None)]),
        vec![1],
        "a slot poisoned before its first vote is still writable"
    );
}

/// An entry that names anything other than the current head is dropped.
///
/// Without this an entry could be built on a superseded ciphertext and put it back in the slot,
/// erasing the vote in between — which a mask needs no signature to publish.
#[test]
fn an_entry_naming_a_stale_parent_is_dropped() {
    assert_eq!(
        select(&[good(1, None), good(1, Some(0)), good(1, Some(0))]),
        vec![1],
        "reaching back past the head cannot erase it"
    );
    assert_eq!(
        select(&[good(1, None), good(1, Some(0)), good(1, None)]),
        vec![1],
        "naming no parent on an occupied slot does not restart the chain"
    );
    assert_eq!(
        select(&[
            good(1, None),
            good(1, Some(0)),
            poisoned(1, Some(0)),
            good(1, Some(1))
        ]),
        vec![3],
        "the head is still 1, so an entry naming 1 is taken"
    );
}

/// A parent belonging to another slot is not this slot's head, so it is dropped. `CRISPProgram`
/// refuses one as well, by keying its commitments on the slot.
#[test]
fn an_entry_naming_another_slots_parent_is_dropped() {
    assert_eq!(
        select(&[good(1, None), good(2, Some(0))]),
        vec![0],
        "slot 2 cannot extend slot 1's entry"
    );
}

/// Two entries naming the same parent are siblings, and only the first usable one is taken.
///
/// This is the cost of the rule, and it is deliberate. An entry that reaches back past the head is
/// indistinguishable from one that was simply built before a sibling landed: both name a parent
/// that is no longer the head, and only the circuit knows whether the entry replaces the slot or
/// adds to it — which is exactly what must stay hidden. Favouring the earlier sibling means a
/// re-vote can be front-run into being dropped, which the voter can see and retry. Favouring the
/// later one would mean a mask built on a superseded ciphertext could restore it over a vote, which
/// is a silent tally corruption nobody can undo.
#[test]
fn a_later_sibling_is_dropped_rather_than_allowed_to_rewind() {
    assert_eq!(
        select(&[good(1, None), good(1, Some(0)), good(1, Some(0))]),
        vec![1],
        "the first entry to extend the head keeps it"
    );
}

/// Sequences interleaved across slots must not bleed into one another.
#[test]
fn interleaved_slots_resolve_independently() {
    let entries = [
        good(1, None),
        poisoned(2, None),
        poisoned(1, Some(0)),
        good(2, None),
    ];

    assert_eq!(
        select(&entries),
        vec![0, 3],
        "each slot resolves on its own entries, in tree order"
    );
}

/// An entry whose metadata is not a slot and a parent cannot be placed in a chain, so it is not
/// selected. It still contributes a leaf — that is the crate's guarantee, not the policy's.
#[test]
fn an_entry_without_valid_metadata_is_not_selected() {
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

    assert!(chain_head_per_slot(&inputs).is_empty());
}
