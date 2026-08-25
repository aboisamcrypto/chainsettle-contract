#![cfg(test)]

extern crate std;

// Oracle failover chain tests: primary → secondary → 30-min TWAP.

use crate::constants::{
    MAX_PRICE_STALENESS_SECS, TWAP_MIN_SPAN_SECS, TWAP_SNAPSHOT_INTERVAL_SECS, TWAP_WINDOW_SECS,
};
use crate::test::test_common::{advance_time, setup, Setup, DEFAULT_THRESHOLD_BPS, PRICE_ONE};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Symbol;

/// Source symbol equality helper.
fn src(s: &Setup, sym: &str) -> bool {
    let r = s.client().preview_price(&s.coll_id);
    r.source == Symbol::new(&s.env, sym)
}

#[test]
fn test_primary_oracle_used_when_healthy() {
    let s = setup();
    assert!(src(&s, "primary"));
    // Price matches the mock's answer.
    assert_eq!(s.client().preview_price(&s.coll_id).price, PRICE_ONE);
}

#[test]
fn test_failover_to_secondary_when_primary_down() {
    let s = setup();
    // Remove the primary entirely — the chain must serve from secondary.
    s.client().set_oracles(
        &s.admin,
        &None::<soroban_sdk::Address>,
        &Some(s.secondary.clone()),
    );
    assert!(src(&s, "secondary"));
}

#[test]
fn test_failover_to_secondary_on_primary_stale() {
    let s = setup();
    // Primary answers with stale data → rejected → secondary serves.
    s.primary_oracle()
        .set_stale(&s.coll_id, &(MAX_PRICE_STALENESS_SECS + 10));
    let r = s.client().preview_price(&s.coll_id);
    assert_eq!(r.source, Symbol::new(&s.env, "secondary"));
    assert_eq!(r.price, PRICE_ONE);

    // The debt asset is still fresh on primary.
    let d = s.client().preview_price(&s.debt_id);
    assert_eq!(d.source, Symbol::new(&s.env, "primary"));
}

#[test]
fn test_stale_primary_answer_rejected() {
    let s = setup();
    s.primary_oracle()
        .set_stale(&s.coll_id, &(MAX_PRICE_STALENESS_SECS + 1));
    assert_eq!(
        s.client().preview_price(&s.coll_id).source,
        Symbol::new(&s.env, "secondary")
    );
}

#[test]
fn test_future_skew_rejected() {
    let s = setup();
    // An answer timestamped far in the future is implausible and must be
    // skipped in favour of the next source.
    let now = s.env.ledger().timestamp();
    s.primary_oracle()
        .set_raw(&s.coll_id, &PRICE_ONE, &(now + 3_600));
    assert_eq!(
        s.client().preview_price(&s.coll_id).source,
        Symbol::new(&s.env, "secondary")
    );
}

#[test]
fn test_twap_fallback_when_both_oracles_dead() {
    let s = setup();
    // Seed a healthy TWAP history: record every snapshot interval at a
    // rising price so the weighted average is well-defined.
    let mut price: i128 = PRICE_ONE;
    for i in 0..12u64 {
        if i > 0 {
            price += 1_000_000; // +0.01 USD per step
            s.primary_oracle().set_price(&s.coll_id, &price);
        }
        advance_time(&s.env, TWAP_SNAPSHOT_INTERVAL_SECS);
        s.client().record_price(&s.coll_id);
    }
    let snaps = s.client().get_twap(&s.coll_id);
    std::assert!(snaps.len() >= 2);

    // Kill both oracles (stale beyond limit) and jump past the min span so
    // the trailing window is usable.
    advance_time(&s.env, TWAP_MIN_SPAN_SECS + TWAP_SNAPSHOT_INTERVAL_SECS);
    s.primary_oracle()
        .set_stale(&s.coll_id, &(MAX_PRICE_STALENESS_SECS + 5));
    s.secondary_oracle()
        .set_stale(&s.coll_id, &(MAX_PRICE_STALENESS_SECS + 5));

    let r = s.client().preview_price(&s.coll_id);
    assert_eq!(r.source, Symbol::new(&s.env, "twap"));

    // TWAP must land inside the observed price band [first, last].
    assert!(r.price >= PRICE_ONE && r.price <= price);

    // Fallback counter incremented.
    let (_, _, fallbacks) = s.client().get_pool_info();
    std::assert!(fallbacks >= 1);
}

#[test]
fn test_no_valid_price_source_panics() {
    let s = setup();
    // No oracles configured and no history for a brand-new asset.
    let unknown = soroban_sdk::Address::generate(&s.env);
    let res = s.client().try_preview_price(&unknown);
    assert!(res.is_err(), "expected 'no valid price source' panic");
}

#[test]
fn test_twap_requires_minimum_span() {
    let s = setup();
    // Build a short history (a few snapshots), then remove both oracles.
    for _ in 0..3 {
        advance_time(&s.env, TWAP_SNAPSHOT_INTERVAL_SECS);
        s.client().record_price(&s.coll_id);
    }
    s.client().set_oracles(
        &s.admin,
        &None::<soroban_sdk::Address>,
        &None::<soroban_sdk::Address>,
    );

    // Immediately after failure the usable span is far below the minimum,
    // so the engine refuses rather than serving a gamed short average.
    let res = s.client().try_preview_price(&s.coll_id);
    assert!(
        res.is_err(),
        "TWAP must refuse to serve without minimum span"
    );

    // Once enough ledger time passes, the trailing span satisfies the
    // minimum and the seeded history becomes usable.
    advance_time(&s.env, TWAP_MIN_SPAN_SECS + 1);
    let r = s.client().preview_price(&s.coll_id);
    assert_eq!(r.source, Symbol::new(&s.env, "twap"));
    assert_eq!(r.price, PRICE_ONE);
}

#[test]
fn test_zero_and_negative_prices_rejected() {
    let s = setup();
    // Build a healthy history deep enough to satisfy the minimum span so
    // the TWAP fallback can legally serve once direct sources die.
    for _ in 0..7 {
        advance_time(&s.env, TWAP_SNAPSHOT_INTERVAL_SECS);
        s.client().record_price(&s.coll_id);
    }
    // Primary reports zero → must be rejected.
    s.primary_oracle().set_price(&s.coll_id, &0i128);
    // Secondary still serves a fresh, sane answer.
    s.secondary_oracle().set_price(&s.coll_id, &PRICE_ONE);
    assert_eq!(
        s.client().preview_price(&s.coll_id).source,
        Symbol::new(&s.env, "secondary")
    );
    // Negative price is likewise rejected.
    s.secondary_oracle().set_price(&s.coll_id, &-5i128);
    // Both direct sources unusable → TWAP path (history seeded above).
    let r = s.client().preview_price(&s.coll_id);
    assert_eq!(r.source, Symbol::new(&s.env, "twap"));
    // TWAP reflects previously seeded healthy prices.
    assert!(r.price > 0);
}

#[test]
fn test_window_cutoff_excludes_old_snapshots() {
    let s = setup();
    // Record at p=2.0, then much later switch to 3.0 and take several
    // snapshots; the 30-minute window must weight only recent data once
    // old points age out.
    s.primary_oracle().set_price(&s.coll_id, &(2 * PRICE_ONE));
    advance_time(&s.env, TWAP_WINDOW_SECS);
    s.primary_oracle().set_price(&s.coll_id, &(3 * PRICE_ONE));
    for _ in 0..3 {
        advance_time(&s.env, TWAP_SNAPSHOT_INTERVAL_SECS);
        s.client().record_price(&s.coll_id);
    }

    // Kill both oracles, then wait just over min-span: the surviving
    // window contains only ~3.0-region observations.
    advance_time(&s.env, TWAP_MIN_SPAN_SECS + TWAP_SNAPSHOT_INTERVAL_SECS);
    s.primary_oracle()
        .set_stale(&s.coll_id, &(MAX_PRICE_STALENESS_SECS + 5));
    s.secondary_oracle()
        .set_stale(&s.coll_id, &(MAX_PRICE_STALENESS_SECS + 5));

    let twap = s.client().twap_now(&s.coll_id);
    // Should be near 3.0, definitely not dragged toward the 1.0/2.0 eras.
    assert!(twap >= 2 * PRICE_ONE, "window cutoff failed: twap={}", twap);
}

#[test]
fn test_market_threshold_config_visible() {
    let s = setup();
    let m = s.client().get_market_config();
    assert_eq!(m.liq_threshold_bps, DEFAULT_THRESHOLD_BPS);
    assert_eq!(m.collateral_token, s.coll_id);
    assert_eq!(m.debt_token, s.debt_id);
}
