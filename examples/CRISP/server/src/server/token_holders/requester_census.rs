// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! Requester-provided census.
//!
//! The default census is reconstructed from Etherscan transfer/delegation logs, which requires an
//! API key, only works on networks Etherscan indexes, and can only ever answer "who holds this
//! token". Applications that already know their own membership — a game with a roster, an
//! allowlisted cohort, a committee — can answer that question directly and exactly.
//!
//! When a round declares `CensusMode::ByRequester`, the requester contract is asked for its
//! electorate via `getCensus(uint256 e3Id) returns (address[])` and the result is used verbatim.
//! The probe is a single `eth_call` at chain head — see the comment on the call for why it is not
//! pinned, and for the immutability that requires of implementors.
//!
//! There is no fallback. A round that declared `ByRequester` and cannot be answered is an error,
//! not a round that quietly becomes a token vote with the wrong electorate.
//!
//! Note this returns *voters*, not ballot options. A round where the two differ (a jury voting on
//! finalists, say) still gets its option list from the E3's `numOptions` and whatever mapping the
//! requester publishes separately.

use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::sol;
use std::collections::HashSet;
use std::time::Duration;

/// Ceiling on the census `eth_call`. A requester that cannot answer within this is treated the same
/// as one that reverts: no census, which the caller turns into a hard error.
const CENSUS_CALL_TIMEOUT: Duration = Duration::from_secs(30);

sol! {
    #[sol(rpc)]
    contract ICensusProvider {
        function getCensus(uint256 e3Id) external view returns (address[]);
    }
}

/// Reads the eligible voter set from the E3 requester.
///
/// Returns `None` when the requester does not implement `getCensus`, the call reverts, or the
/// result is empty. The caller decides what that means: under `CensusMode::ByRequester` it is a
/// hard error, because the round declared an electorate nobody can produce.
pub async fn try_fetch_requester_census(
    requester: Address,
    e3_id: u64,
    rpc_url: &str,
) -> Option<Vec<Address>> {
    let url = match rpc_url.parse() {
        Ok(url) => url,
        Err(e) => {
            log::warn!("[e3_id={}] Invalid RPC URL for census probe: {}", e3_id, e);
            return None;
        }
    };

    let provider = ProviderBuilder::new().connect_http(url);
    let contract = ICensusProvider::new(requester, provider);

    // Read at head rather than pinned to a historical block, for two reasons.
    //
    // First, there is no block number to pin to: the E3's `requestBlock` is an EIP-6372 *timestamp*,
    // not a height, so passing it to `eth_call` asks for a block billions high and the node answers
    // `BlockOutOfRangeError`.
    //
    // Second, pinning is unnecessary. The census is addressed by `e3Id`, and the contract fixes it
    // when the round opens — the answer cannot change afterwards, so head and snapshot agree. This
    // is a requirement on implementors, not an assumption: `getCensus(e3Id)` MUST be immutable once
    // the round is open, or ballots already cast against one eligibility tree would be validated
    // against another.
    // Alloy sets no default request timeout, so an unresponsive RPC endpoint would hang this probe
    // indefinitely and stall the indexer on the E3 that triggered it.
    // Bound to a `let` so the builder outlives the future that borrows it.
    let call_builder = contract.getCensus(alloy::primitives::U256::from(e3_id));

    let census = match tokio::time::timeout(CENSUS_CALL_TIMEOUT, call_builder.call()).await {
        Ok(Ok(census)) => census,
        Ok(Err(e)) => {
            log::info!(
                "[e3_id={}] Requester {} did not answer getCensus ({})",
                e3_id,
                requester,
                e
            );
            return None;
        }
        Err(_) => {
            log::warn!(
                "[e3_id={}] Requester {} census probe timed out after {:?}",
                e3_id,
                requester,
                CENSUS_CALL_TIMEOUT
            );
            return None;
        }
    };

    let Some(sanitized) = sanitize_census(census) else {
        log::warn!(
            "[e3_id={}] Requester {} returned no usable census addresses",
            e3_id,
            requester
        );
        return None;
    };

    log::info!(
        "[e3_id={}] Using requester-provided census from {}: {} eligible addresses",
        e3_id,
        requester,
        sanitized.len()
    );

    Some(sanitized)
}

/// Drops zero addresses and duplicates from a requester-supplied census.
///
/// A duplicated address would get two Merkle leaves and therefore two ballots, so this dedupes
/// rather than trusting the requester to have done it. First-seen order is preserved so the leaf
/// ordering is deterministic for a given census. Returns `None` when nothing usable remains.
fn sanitize_census(census: Vec<Address>) -> Option<Vec<Address>> {
    let mut seen = HashSet::new();
    let deduped: Vec<Address> = census
        .into_iter()
        .filter(|address| !address.is_zero() && seen.insert(*address))
        .collect();

    (!deduped.is_empty()).then_some(deduped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: u8) -> Address {
        Address::from([byte; 20])
    }

    #[test]
    fn keeps_distinct_addresses_in_order() {
        let census = vec![addr(3), addr(1), addr(2)];
        assert_eq!(
            sanitize_census(census),
            Some(vec![addr(3), addr(1), addr(2)])
        );
    }

    #[test]
    fn removes_duplicates_keeping_first_occurrence() {
        let census = vec![addr(1), addr(2), addr(1), addr(2), addr(3)];
        assert_eq!(
            sanitize_census(census),
            Some(vec![addr(1), addr(2), addr(3)])
        );
    }

    #[test]
    fn removes_zero_addresses() {
        let census = vec![addr(1), Address::ZERO, addr(2)];
        assert_eq!(sanitize_census(census), Some(vec![addr(1), addr(2)]));
    }

    #[test]
    fn empty_census_is_rejected() {
        assert_eq!(sanitize_census(vec![]), None);
    }

    #[test]
    fn census_of_only_zero_addresses_is_rejected() {
        assert_eq!(sanitize_census(vec![Address::ZERO, Address::ZERO]), None);
    }
}
