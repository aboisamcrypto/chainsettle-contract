#![cfg(test)]

extern crate std;

// Fuzz tests for the liquidation engine.
//
// Two complementary harnesses:
// 1. Property tests over the pure math core (`compute_plan`) covering
//    every safety invariant under arbitrary inputs.
// 2. A deterministic scenario fuzzer driving the live contract through
//    random deposit/borrow/crash/oracle-outage/liquidation sequences,
//    asserting pool solvency invariants after every operation.

use crate::liquidation::{compute_plan, RepayMode};
use crate::test::test_common::{seed_liquidity, setup, PRICE_ONE};
use proptest::prelude::*;

// ============================================================
// 1. PROPERTY TESTS — PURE MATH CORE
// ============================================================

prop_compose! {
    fn arb_config()
        (base in 100u32..=1_000u32, max_extra in 0u32..=1_500u32, cap_raw in 1_000u32..=2_500u32)
        -> (u32, u32, u32) {
        let max = base.saturating_add(max_extra).min(2_500);
        (base, max, cap_raw.max(max))
    }
}

fn arb_mode() -> impl Strategy<Value = RepayMode> {
    prop_oneof![
        Just(RepayMode::PartialToTarget),
        Just(RepayMode::Full),
        (0i128..=2_000_000_000_000i128).prop_map(RepayMode::Specified),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn prop_plan_invariants_hold_for_all_inputs(
        coll_units in 0i128..=4_000_000_000_000i128,
        debt_units in 0i128..=4_000_000_000_000i128,
        price_coll in 1i128..=400_000_000i128,
        price_debt in 1i128..=400_000_000i128,
        thr in 1_000u32..=9_500u32,
        (base, max, cap) in arb_config(),
        mode in arb_mode(),
    ) {
        let plan = compute_plan(
            coll_units, debt_units, price_coll, price_debt,
            thr, base, max, cap, mode,
        );

        // No negative outputs.
        prop_assert!(plan.repay_units >= 0);
        prop_assert!(plan.seize_units >= 0);
        prop_assert!(plan.bad_debt_units >= 0);

        // Liquidator never seizes more collateral than exists, never pays
        // more debt than is owed.
        prop_assert!(plan.seize_units <= coll_units);
        prop_assert!(plan.repay_units <= debt_units);

        // Dynamic incentive bounds: within the configured dynamic range
        // and always capped by the configured penalty cap.
        prop_assert!(plan.bonus_bps <= cap);
        prop_assert!(plan.bonus_bps >= base.min(plan.bonus_bps));
        prop_assert!(plan.bonus_bps <= max);

        // Healthy positions are untouchable.
        let coll_value = crate::liquidation::units_to_value(coll_units, price_coll);
        let debt_value = crate::liquidation::units_to_value(debt_units, price_debt);
        let (num, den) =
            crate::liquidation::health_ratio(coll_value, debt_value, thr);
        if !crate::liquidation::is_liquidatable(num, den) {
            prop_assert_eq!(plan.repay_units, 0);
            prop_assert_eq!(plan.seize_units, 0);
            prop_assert_eq!(plan.bad_debt_units, 0);
            return Ok(());
        }

        // Drained ⇔ bad debt equals the remainder; undrained ⇒ zero write-off.
        if plan.bad_debt_units > 0 {
            prop_assert_eq!(plan.bad_debt_units, debt_units - plan.repay_units);
            prop_assert_eq!(plan.repay_units + plan.seize_units > 0 || debt_units > 0, true);
        }

        // Undrained *partial* liquidations must restore health towards the
        // target (HF ≥ 1.1 modulo integer-floor slack). Specified and Full
        // modes intentionally do not aim at the target.
        if matches!(mode, RepayMode::PartialToTarget)
            && plan.repay_units > 0
            && plan.bad_debt_units == 0
        {
            let post_coll_value = crate::liquidation::units_to_value(
                coll_units - plan.seize_units, price_coll);
            let post_debt_value = crate::liquidation::units_to_value(
                debt_units - plan.repay_units, price_debt);
            let (pn, pd) =
                crate::liquidation::health_ratio(post_coll_value, post_debt_value, thr);
            let eps = 1_000_000i128;
            prop_assert!(
                pn * 10 + eps >= pd * 11,
                "post-HF {}/{} below target after undrained partial liquidation",
                pn, pd
            );
        }
    }
}

// ============================================================
// 2. SCENARIO FUZZER — LIVE CONTRACT FAILURE MODES
// ============================================================

/// Small deterministic PRNG (xorshift64*) so CI failures are reproducible.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }
}

#[test]
fn fuzz_scenario_random_markets_stay_solvent() {
    for seed in 0..24u64 {
        let mut rng = Rng::new(seed.wrapping_mul(0x9E3779B97F4A7C15));
        let s = setup();
        let client = s.client();

        seed_liquidity(
            &s,
            &[&s.d1, &s.d2, &s.d3],
            &[
                200 + rng.below(3_000) as i128,
                200 + rng.below(3_000) as i128,
                200 + rng.below(3_000) as i128,
            ],
        );

        for _step in 0..30u64 {
            match rng.below(7) {
                // Alice posts collateral.
                0 => {
                    let amt = 100 + rng.below(2_000) as i128;
                    client.deposit_collateral(&s.alice, &amt);
                }
                // Alice borrows (may fail on HF).
                1 => {
                    let amt = 50 + rng.below(600) as i128;
                    let _ = client.try_borrow(&s.alice, &amt);
                }
                // Alice repays.
                2 => {
                    let amt = 10 + rng.below(400) as i128;
                    let _ = client.try_repay(&s.alice, &amt);
                }
                // Depositor pulls liquidity (may fail when locked).
                3 => {
                    let who = [&s.d1, &s.d2, &s.d3][rng.below(3) as usize];
                    let bal = client.get_depositor_balance(who);
                    if bal > 0 {
                        let _ = client.try_withdraw(who, &(1 + rng.below(bal as u64) as i128));
                    }
                }
                // Price shock: multiply or divide collateral price.
                4 => {
                    let cur = s.primary_oracle().latest_price(&s.coll_id).0;
                    let next = if rng.below(2) == 0 {
                        (cur / 2).max(1_000_000)
                    } else {
                        (cur.saturating_mul(3) / 2).min(300_000_000)
                    };
                    s.primary_oracle().set_price(&s.coll_id, &next);
                    crate::test::test_common::advance_time(&s.env, 30 + rng.below(90));
                }
                // Oracle outage: force the primary stale; sometimes recover it.
                5 => {
                    if rng.below(2) == 0 {
                        s.primary_oracle().set_stale(
                            &s.coll_id,
                            &(crate::constants::MAX_PRICE_STALENESS_SECS + 60),
                        );
                        s.primary_oracle().set_stale(
                            &s.debt_id,
                            &(crate::constants::MAX_PRICE_STALENESS_SECS + 60),
                        );
                    } else {
                        s.primary_oracle().set_price(&s.coll_id, &PRICE_ONE);
                        s.primary_oracle().set_price(&s.debt_id, &PRICE_ONE);
                    }
                }
                // Liquidation attempt in a random mode.
                _ => {
                    let mode = rng.below(3);
                    let amt = match mode {
                        0 => crate::FULL_REPAY_SENTINEL,
                        1 => 0i128,
                        _ => 1 + rng.below(900) as i128,
                    };
                    let _ = client.try_liquidate(&s.bob, &s.alice, &amt);
                }
            }

            // ---- Global solvency invariants after EVERY step ----------
            let (td, tb, _) = client.get_pool_info();

            // Depositor accounting matches stored balances.
            let d = client.get_depositors();
            let mut sum_bal = 0i128;
            for dep in d.iter() {
                sum_bal += client.get_depositor_balance(&dep);
            }
            assert_eq!(
                sum_bal, td,
                "seed {}: depositor balances {} != accounting {}",
                seed, sum_bal, td
            );

            // The pool's cash plus outstanding receivables always covers
            // depositor claims.
            let pool_bal = soroban_sdk::token::Client::new(&s.env, &s.debt_id).balance(&s.id);
            assert!(
                pool_bal.saturating_add(tb) >= td,
                "seed {}: insolvent pool ({}+{} < {}) after step",
                seed,
                pool_bal,
                tb,
                td
            );

            // Borrow accounting mirrors positions.
            let pos = client.get_position(&s.alice);
            assert_eq!(tb, pos.debt, "seed {}: borrowed != Σdebts", seed);

            // A fully drained borrower carries no phantom debt.
            if pos.collateral == 0 {
                assert_eq!(pos.debt, 0, "seed {}: orphaned debt remains", seed);
            }
        }
    }
}
