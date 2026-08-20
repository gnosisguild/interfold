// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

//! A sliding-window rate limiter for the vote relay.
//!
//! `/voting/broadcast` signs and pays for a transaction on behalf of whoever calls it, so an
//! unthrottled caller can spend the relay's funds as fast as it can post proofs. Two windows
//! bound that: a per-caller window against one hot client, and a global window that caps what
//! the relay key can spend regardless of how many addresses the traffic arrives from.
//!
//! The limits are deliberately generous for people and tight for loops: one honest ballot costs
//! minutes of client-side proving, so a human cannot reach them.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Broadcasts one caller may relay per window.
const PER_CALLER_LIMIT: usize = 10;

/// Broadcasts the relay accepts per window across all callers.
const GLOBAL_LIMIT: usize = 120;

/// The sliding window both limits are counted over.
const WINDOW: Duration = Duration::from_secs(60);

/// Why a request was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum RateLimitExceeded {
    /// The caller sent more than [`PER_CALLER_LIMIT`] requests inside the window.
    Caller,
    /// The relay as a whole received more than [`GLOBAL_LIMIT`] requests inside the window.
    Global,
}

/// A shared sliding-window limiter. Cheap to clone via `web::Data`; one instance must be built
/// outside the `HttpServer` factory closure, or every worker gets its own counters and the
/// effective limit multiplies by the worker count.
pub struct RateLimiter {
    state: Mutex<State>,
}

struct State {
    /// Recent request instants per caller. Pruned on every check, so a caller that goes quiet
    /// costs nothing after one window.
    per_caller: HashMap<String, Vec<Instant>>,
    /// Recent request instants across all callers.
    global: Vec<Instant>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                per_caller: HashMap::new(),
                global: Vec::new(),
            }),
        }
    }

    /// Record one request from `caller` and answer whether it is within both windows.
    ///
    /// A refused request is not recorded, so being refused does not extend the refusal.
    pub fn check(&self, caller: &str) -> Result<(), RateLimitExceeded> {
        self.check_at(caller, Instant::now())
    }

    fn check_at(&self, caller: &str, now: Instant) -> Result<(), RateLimitExceeded> {
        let cutoff = now.checked_sub(WINDOW);
        let within = |t: &Instant| cutoff.is_none_or(|c| *t > c);

        let mut state = self.state.lock().expect("rate limiter mutex poisoned");

        state.global.retain(|t| within(t));
        if state.global.len() >= GLOBAL_LIMIT {
            return Err(RateLimitExceeded::Global);
        }

        // Prune every caller, not only this one, so idle callers do not accumulate forever.
        state.per_caller.retain(|_, times| {
            times.retain(|t| within(t));
            !times.is_empty()
        });

        let times = state.per_caller.entry(caller.to_string()).or_default();
        if times.len() >= PER_CALLER_LIMIT {
            return Err(RateLimitExceeded::Caller);
        }

        times.push(now);
        state.global.push(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_the_caller_limit_and_refuses_the_next() {
        let limiter = RateLimiter::new();
        let now = Instant::now();

        for _ in 0..PER_CALLER_LIMIT {
            assert_eq!(limiter.check_at("a", now), Ok(()));
        }
        assert_eq!(limiter.check_at("a", now), Err(RateLimitExceeded::Caller));

        // Another caller is unaffected by the first one's refusal.
        assert_eq!(limiter.check_at("b", now), Ok(()));
    }

    #[test]
    fn a_caller_recovers_once_the_window_slides_past() {
        let limiter = RateLimiter::new();
        let start = Instant::now();

        for _ in 0..PER_CALLER_LIMIT {
            assert_eq!(limiter.check_at("a", start), Ok(()));
        }
        assert_eq!(limiter.check_at("a", start), Err(RateLimitExceeded::Caller));

        let later = start + WINDOW + Duration::from_secs(1);
        assert_eq!(limiter.check_at("a", later), Ok(()));
    }

    #[test]
    fn the_global_window_caps_traffic_across_callers() {
        let limiter = RateLimiter::new();
        let now = Instant::now();

        // Enough distinct callers to stay under every per-caller limit.
        for i in 0..GLOBAL_LIMIT {
            let caller = format!("caller-{}", i / (PER_CALLER_LIMIT - 1));
            assert_eq!(limiter.check_at(&caller, now), Ok(()));
        }
        assert_eq!(
            limiter.check_at("one-more", now),
            Err(RateLimitExceeded::Global)
        );
    }

    #[test]
    fn a_refused_request_is_not_counted() {
        let limiter = RateLimiter::new();
        let start = Instant::now();

        for _ in 0..PER_CALLER_LIMIT {
            assert_eq!(limiter.check_at("a", start), Ok(()));
        }
        // Hammering while refused must not push the recovery point further out.
        for _ in 0..100 {
            assert_eq!(limiter.check_at("a", start), Err(RateLimitExceeded::Caller));
        }

        let later = start + WINDOW + Duration::from_secs(1);
        assert_eq!(limiter.check_at("a", later), Ok(()));
    }
}
