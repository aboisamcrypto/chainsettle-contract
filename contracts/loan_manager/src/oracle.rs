//! Oracle failover chain: primary → secondary → 30-minute TWAP.
//!
//! Direct oracle answers are validated for freshness and positivity. Every
//! valid answer feeds a cumulative-price accumulator whose snapshots power
//! the time-weighted average fallback used when both oracles are down or
//! returning stale data.
#![allow(dead_code)]

use crate::constants::{
    MAX_FUTURE_SKEW_SECS, MAX_PRICE_STALENESS_SECS, PRICE_SCALE, TWAP_MAX_SNAPSHOTS,
    TWAP_MIN_SPAN_SECS, TWAP_SNAPSHOT_INTERVAL_SECS, TWAP_WINDOW_SECS,
};
use crate::storage::{
    bump_twap_fallback_count, get_primary_oracle, get_secondary_oracle, get_twap_state, set,
    DataKey, PriceResolution,
};
use soroban_sdk::{contractclient, Address, Env, Symbol};

// ============================================================
// EXTERNAL ORACLE INTERFACE
// ============================================================

/// Minimal price-feed interface expected of oracle contracts.
/// Returns `(price, timestamp)` where price uses [`PRICE_SCALE`] decimals
/// and timestamp is the unix-seconds observation time.
#[contractclient(name = "OracleClient")]
pub trait Oracle {
    fn latest_price(env: Env, asset: Address) -> (i128, u64);
}

// ============================================================
// VALIDATION
// ============================================================

/// Validate a raw oracle answer: price must be positive and the observation
/// neither stale nor implausibly in the future.
pub fn validate_answer(price: i128, ts: u64, now: u64) -> bool {
    if price <= 0 {
        return false;
    }
    // Reject answers stamped far in the future (clock skew attack).
    if ts > now.saturating_add(MAX_FUTURE_SKEW_SECS) {
        return false;
    }
    // Reject stale answers.
    if now.saturating_sub(ts) > MAX_PRICE_STALENESS_SECS {
        return false;
    }
    true
}

// ============================================================
// FAILOVER CHAIN
// ============================================================

fn try_fetch(env: &Env, oracle: &Address, asset: &Address) -> Option<i128> {
    let client = OracleClient::new(env, oracle);
    match client.try_latest_price(asset) {
        Ok(Ok((price, ts))) => {
            let now = env.ledger().timestamp();
            if validate_answer(price, ts, now) {
                Some(price)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Resolve a trustworthy price for `asset`, walking the failover chain:
/// primary oracle → secondary oracle → 30-minute TWAP accumulator.
///
/// Panics when no source is available ("no valid price source").
pub fn resolve_price(env: &Env, asset: &Address) -> PriceResolution {
    let now = env.ledger().timestamp();

    if let Some(primary) = get_primary_oracle(env) {
        if let Some(price) = try_fetch(env, &primary, asset) {
            record_observation(env, asset, price, now);
            return PriceResolution {
                price,
                source: Symbol::new(env, "primary"),
            };
        }
    }

    if let Some(secondary) = get_secondary_oracle(env) {
        if let Some(price) = try_fetch(env, &secondary, asset) {
            record_observation(env, asset, price, now);
            return PriceResolution {
                price,
                source: Symbol::new(env, "secondary"),
            };
        }
    }

    // Both direct sources failed — fall back to the 30-minute TWAP built
    // from previously recorded observations.
    bump_twap_fallback_count(env);
    let price = twap_price(env, asset, now);
    PriceResolution {
        price,
        source: Symbol::new(env, "twap"),
    }
}

// ============================================================
// TWAP ACCUMULATOR
// ============================================================

/// Feed an observed price into the cumulative-price accumulator and record
/// a bounded ring-buffer snapshot on the configured cadence.
pub fn record_observation(env: &Env, asset: &Address, price: i128, now: u64) {
    let mut st = get_twap_state(env, asset);

    // Unseeded state is signalled by a zero price (valid prices are always
    // positive); `last_ts == 0` cannot be used because ledger time may
    // legitimately start at zero.
    if st.last_price == 0 || st.last_ts >= now {
        // Seed (or same-ledger duplicate): just remember the price.
        st.last_ts = now;
        st.last_price = price;
        set(env, DataKey::Twap(asset.clone()), &st);
        return;
    }

    // Accumulate price * elapsed at the previous price before advancing.
    st.cumulative = st.cumulative.saturating_add(
        st.last_price
            .saturating_mul((now - st.last_ts).min(u32::MAX as u64) as i128),
    );
    st.last_ts = now;
    st.last_price = price;

    if st.snapshots.is_empty()
        || now
            >= st
                .last_snapshot_ts
                .saturating_add(TWAP_SNAPSHOT_INTERVAL_SECS)
    {
        st.snapshots.push_back(crate::storage::PriceObservation {
            ts: now,
            cum: st.cumulative,
        });
        while st.snapshots.len() > TWAP_MAX_SNAPSHOTS {
            st.snapshots.remove(0);
        }
        st.last_snapshot_ts = now;
    }

    set(env, DataKey::Twap(asset.clone()), &st);
}

/// Time-weighted average price over the trailing [`TWAP_WINDOW_SECS`].
///
/// Panics when there is insufficient recorded history
/// ("no valid price source").
pub fn twap_price(env: &Env, asset: &Address, now: u64) -> i128 {
    let st = get_twap_state(env, asset);

    if st.last_price <= 0 || st.snapshots.len() < 2 {
        panic!("no valid price source");
    }

    // Interpolate the accumulator to `now` using the last observed price.
    let elapsed_now = now.saturating_sub(st.last_ts).min(u32::MAX as u64) as i128;
    let cum_now = st
        .cumulative
        .saturating_add(st.last_price.saturating_mul(elapsed_now));

    // Pick the newest snapshot that still lies inside the window; if every
    // snapshot has aged out of the window we degrade gracefully to the most
    // recent one rather than failing outright.
    let cutoff = now.saturating_sub(TWAP_WINDOW_SECS);
    let mut start_idx = st.snapshots.len() - 1;
    for i in 0..st.snapshots.len() {
        if st.snapshots.get(i).unwrap().ts >= cutoff {
            start_idx = i;
            break;
        }
    }
    let start = st.snapshots.get(start_idx).unwrap();

    let span = now.saturating_sub(start.ts);
    if span < TWAP_MIN_SPAN_SECS {
        panic!("no valid price source");
    }

    let delta = cum_now - start.cum;
    if delta <= 0 {
        panic!("no valid price source");
    }

    delta / span as i128
}

/// Convenience helper used by call sites: resolve through the failover
/// chain and sanity-check the result.
pub fn require_price(env: &Env, asset: &Address) -> i128 {
    let r = resolve_price(env, asset);
    if r.price <= 0 || r.price > PRICE_SCALE.saturating_mul(1_000_000_000) {
        panic!("invalid price");
    }
    r.price
}
