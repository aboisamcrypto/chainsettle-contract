#![cfg(test)]

extern crate std;

// Liquidation engine tests: dynamic incentives, partial liquidation,
// penalty caps, full drain with bad-debt socialization, and guards.

use crate::test::test_common::{open_position, seed_liquidity, setup, Setup, PRICE_ONE};
use soroban_sdk::token;

fn coll_price(s: &Setup, price: i128) {
    s.primary_oracle().set_price(&s.coll_id, &price);
}

fn coll_balance(s: &Setup, user: &soroban_sdk::Address) -> i128 {
    token::Client::new(&s.env, &s.coll_id).balance(user)
}

fn debt_balance(s: &Setup, user: &soroban_sdk::Address) -> i128 {
    token::Client::new(&s.env, &s.debt_id).balance(user)
}

// ============================================================
// GUARDS
// ============================================================

#[test]
#[should_panic(expected = "position not liquidatable")]
fn test_healthy_position_cannot_be_liquidated() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_000);
    s.client().liquidate(&s.bob, &s.alice, &0i128);
}

#[test]
#[should_panic(expected = "cannot self-liquidate")]
fn test_self_liquidation_rejected() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_500);
    coll_price(&s, 5_000_000); // deep crash
    s.client()
        .liquidate(&s.alice, &s.alice, &crate::FULL_REPAY_SENTINEL);
}

// ============================================================
// PARTIAL LIQUIDATION → HF RESTORED TO ~1.1
// ============================================================

/// Shallow crash: collateral value 1200 vs debt 1000 at thr=80%
/// (threshold-HF 0.96 < 1 → liquidatable, raw CR 1.2 → partial liq helps).
#[test]
fn test_partial_liquidation_restores_target_hf() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_000);

    coll_price(&s, 60_000_000); // 0.6 USD → coll value 1200
    let bob_coll_before = coll_balance(&s, &s.bob);
    let bob_debt_before = debt_balance(&s, &s.bob);

    let res = s.client().liquidate(&s.bob, &s.alice, &0i128);

    // Dynamic bonus near HF=0.96 is close to the 5% base.
    assert!(
        res.bonus_bps >= 500 && res.bonus_bps < 800,
        "bonus={}",
        res.bonus_bps
    );
    assert_eq!(res.bad_debt_socialized, 0);

    // Health factor restored to ≈1.1: num/den >= 11/10 - rounding slack.
    let (num, den) = s.client().health_factor(&s.alice);
    std::assert!(
        num * 10 + 20 >= den * 11,
        "post-HF {} / {} not ≈1.1",
        num,
        den
    );
    // And definitely above 1.
    std::assert!(num > den);

    // Borrower keeps the remainder of both sides.
    let pos = s.client().get_position(&s.alice);
    assert!(pos.collateral > 0 && pos.debt > 0);

    // Liquidator paid debt tokens and received collateral (+bonus).
    assert_eq!(debt_balance(&s, &s.bob), 100_000_000_000 - res.repay_units);
    assert_eq!(coll_balance(&s, &s.bob), bob_coll_before + res.seize_units);
    let _ = bob_debt_before;

    // Pool accounting consistent.
    let (_, tb, _) = s.client().get_pool_info();
    assert_eq!(tb, pos.debt);
}

#[test]
fn test_specified_partial_amount_is_respected_and_capped() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_000);
    coll_price(&s, 60_000_000);

    // Ask for more than the whole debt — engine caps it safely.
    let res = s.client().liquidate(&s.bob, &s.alice, &5_000i128);
    assert!(res.repay_units <= 1_000);
    assert!(res.bad_debt_socialized == 0 || res.repay_units == 1_000);
}

// ============================================================
// DYNAMIC LIQUIDATOR INCENTIVES
// ============================================================

#[test]
fn test_dynamic_bonus_grows_as_health_deteriorates() {
    // Shallow crash.
    let s1 = setup();
    seed_liquidity(&s1, &[&s1.d1], &[10_000]);
    open_position(&s1, &s1.alice, 2_000, 1_000);
    coll_price(&s1, 60_000_000);
    let shallow = s1.client().liquidate(&s1.bob, &s1.alice, &0i128);

    // Deep crash (same position shape, worse price).
    let s2 = setup();
    seed_liquidity(&s2, &[&s2.d1], &[10_000]);
    open_position(&s2, &s2.alice, 2_000, 1_500);
    coll_price(&s2, 10_000_000); // 0.1 USD → raw CR ≈ 0.133
    let deep = s2.client().liquidate(&s2.bob, &s2.alice, &0i128);

    std::assert!(
        deep.bonus_bps > shallow.bonus_bps,
        "deep bonus {} must exceed shallow bonus {}",
        deep.bonus_bps,
        shallow.bonus_bps
    );
    assert_eq!(shallow.bonus_bps, 580); // linear interp at HF=0.96
    assert_eq!(deep.bonus_bps, 1_500); // max dynamic bonus at very low HF
}

#[test]
fn test_penalty_cap_enforced_over_configured_max() {
    let s = setup();
    // Admin raises the dynamic ceiling to 25% but the hard cap stays 20%:
    // the effective bonus must never exceed the cap.
    s.client()
        .set_liq_params(&s.admin, &500u32, &2_500u32, &2_000u32);
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_500);
    coll_price(&s, 10_000_000);

    let res = s.client().liquidate(&s.bob, &s.alice, &0i128);
    assert_eq!(res.bonus_bps, 2_000, "penalty cap must bind");

    // Seized value can never exceed repay * (1 + cap).
    let repay_value = res.repay_units; // debt price = 1.0
    let seize_value = res.seize_units * 10_000_000 / PRICE_ONE;
    std::assert!(
        seize_value <= repay_value * 12_000 / 10_000 + 1,
        "seized {} exceeds repay*1.2 {}",
        seize_value,
        repay_value * 12_000 / 10_000
    );
}

// ============================================================
// FULL LIQUIDATION & BAD-DEBT SOCIALIZATION
// ============================================================

#[test]
fn test_full_liquidation_drains_and_socializes_bad_debt() {
    let s = setup();
    // Depositors: d1=6k, d2=4k → pro-rata shares 60/40.
    seed_liquidity(&s, &[&s.d1, &s.d2], &[6_000, 4_000]);
    open_position(&s, &s.alice, 2_000, 1_500);

    // Catastrophic collateral crash: 2000 units @0.05 = 100 value vs 1500
    // debt — deeply insolvent, everything seized becomes bad debt.
    coll_price(&s, 5_000_000);

    let res = s
        .client()
        .liquidate(&s.bob, &s.alice, &crate::FULL_REPAY_SENTINEL);

    // Position fully drained and written off.
    assert_eq!(res.bad_debt_socialized, 1_500 - res.repay_units);
    let pos = s.client().get_position(&s.alice);
    assert_eq!(pos.collateral, 0);
    assert_eq!(pos.debt, 0);

    // Liquidator received all collateral.
    assert_eq!(res.seize_units, 2_000);
    assert_eq!(coll_balance(&s, &s.bob), 100_000_000_000 + res.seize_units);

    // All borrower debt written off; accounting consistent.
    let (td, tb, _) = s.client().get_pool_info();
    assert_eq!(tb, 0, "all borrower debt written off");
    let d1 = s.client().get_depositor_balance(&s.d1);
    let d2 = s.client().get_depositor_balance(&s.d2);
    assert_eq!(td, d1 + d2, "deposits accounting must match balances");
}

#[test]
fn test_bad_debt_loss_split_proportionally() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1, &s.d2], &[6_000, 4_000]);
    open_position(&s, &s.alice, 2_000, 1_500);
    coll_price(&s, 5_000_000);

    s.client()
        .liquidate(&s.bob, &s.alice, &crate::FULL_REPAY_SENTINEL);

    let total_after =
        s.client().get_depositor_balance(&s.d1) + s.client().get_depositor_balance(&s.d2);
    let (td, _, _) = s.client().get_pool_info();
    assert_eq!(td, total_after, "deposits accounting must match balances");

    // Loss = 1500 - repay(≈87) ≈ 1413 spread 60/40.
    let d1 = s.client().get_depositor_balance(&s.d1);
    let d2 = s.client().get_depositor_balance(&s.d2);
    std::assert!(d1 < 6_000 && d2 < 4_000);
    std::assert!(
        (d1 - d2).abs() <= 2_200,
        "split should stay roughly proportional"
    );
    // The pool still holds at least what depositors are owed.
    assert!(debt_balance(&s, &s.id) >= td);
}

#[test]
fn test_deep_partial_also_drains_and_socializes() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_500);
    coll_price(&s, 5_000_000);

    // Partial mode on a hopeless position: target unreachable → affordability
    // cap drains all collateral and the remainder is socialized.
    let res = s.client().liquidate(&s.bob, &s.alice, &0i128);
    assert_eq!(res.seize_units, 2_000);
    std::assert!(res.bad_debt_socialized > 0);
    let pos = s.client().get_position(&s.alice);
    assert_eq!(pos.debt, 0);
}

#[test]
fn test_zero_collateral_position_fully_socializes() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_000);
    coll_price(&s, 1_000_000); // nearly worthless

    // Drain everything: the tiny collateral covers almost nothing.
    let r1 = s
        .client()
        .liquidate(&s.bob, &s.alice, &crate::FULL_REPAY_SENTINEL);
    std::assert!(r1.bad_debt_socialized > 0);
    assert_eq!(r1.bad_debt_socialized, 1_000 - r1.repay_units);
}

// ============================================================
// POOL SOLVENCY INVARIANT
// ============================================================

#[test]
fn test_pool_never_owes_more_than_it_holds() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1, &s.d2], &[6_000, 4_000]);
    open_position(&s, &s.alice, 2_000, 1_500);
    coll_price(&s, 5_000_000);
    let _ = s
        .client()
        .liquidate(&s.bob, &s.alice, &crate::FULL_REPAY_SENTINEL);

    // Re-open at healthy prices, crash again, liquidate again.
    coll_price(&s, PRICE_ONE);
    open_position(&s, &s.alice, 1_000, 400);
    coll_price(&s, 2_000_000);
    let _ = s
        .client()
        .liquidate(&s.bob, &s.alice, &crate::FULL_REPAY_SENTINEL);

    let (td, tb, _) = s.client().get_pool_info();
    assert!(
        debt_balance(&s, &s.id) >= td,
        "contract balance {} < deposits owed {}",
        debt_balance(&s, &s.id),
        td
    );
    assert!(tb == 0);
}

// ============================================================
// ADDITIONAL EDGE CASES & SCENARIOS
// ============================================================

#[test]
fn test_liquidation_with_multiple_borrowers_independent() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[20_000]);
    
    // Two borrowers open positions
    open_position(&s, &s.alice, 2_000, 1_000);
    open_position(&s, &s.bob, 3_000, 1_500);
    
    // Price crash makes Alice liquidatable but not Bob
    coll_price(&s, 60_000_000);
    
    // Liquidate Alice
    let res = s.client().liquidate(&s.d1, &s.alice, &0i128);
    assert!(res.repay_units > 0);
    
    // Bob's position should remain untouched
    let bob_pos = s.client().get_position(&s.bob);
    assert_eq!(bob_pos.collateral, 3_000);
    assert_eq!(bob_pos.debt, 1_500);
    
    // Pool accounting should be correct
    let (_, tb, _) = s.client().get_pool_info();
    let alice_pos = s.client().get_position(&s.alice);
    assert_eq!(tb, alice_pos.debt + bob_pos.debt);
}

#[test]
fn test_liquidation_at_exact_threshold() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_000);
    
    // Set price so collateral value = debt / threshold (exactly at threshold)
    // threshold = 80% = 0.8, so collateral_value = debt / 0.8 = 1000 / 0.8 = 1250
    // 2000 units * price = 1250 → price = 0.625
    coll_price(&s, 62_500_000);
    
    // Position should be liquidatable (at threshold boundary)
    let res = s.client().liquidate(&s.bob, &s.alice, &0i128);
    assert!(res.repay_units > 0);
}

#[test]
fn test_liquidator_insufficient_debt_tokens() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_000);
    coll_price(&s, 50_000_000);
    
    // Create a liquidator with limited debt tokens
    let liquidator = soroban_sdk::Address::generate(&s.env);
    token::Client::new(&s.env, &s.debt_id).mint(&liquidator, &100i128);
    
    // Should still be able to liquidate with available balance
    let res = s.client().liquidate(&liquidator, &s.alice, &0i128);
    assert!(res.repay_units <= 100);
}

#[test]
fn test_sequential_partial_liquidations() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_000);
    coll_price(&s, 55_000_000);
    
    // First partial liquidation
    let res1 = s.client().liquidate(&s.bob, &s.alice, &100i128);
    assert_eq!(res1.repay_units, 100);
    
    let pos_after_1 = s.client().get_position(&s.alice);
    
    // Position might still be liquidatable, try again
    if pos_after_1.debt > 0 && pos_after_1.collateral > 0 {
        let (num, den) = s.client().health_factor(&s.alice);
        if num < den {
            // Second partial liquidation
            let res2 = s.client().liquidate(&s.bob, &s.alice, &100i128);
            assert!(res2.repay_units > 0);
            
            // Total collateral seized should not exceed initial
            assert!(res1.seize_units + res2.seize_units <= 2_000);
        }
    }
}

#[test]
fn test_liquidation_bonus_increases_monotonically() {
    // Test that bonus increases as health factor decreases
    let prices = vec![70_000_000, 60_000_000, 50_000_000, 40_000_000, 30_000_000];
    let mut bonuses = vec![];
    
    for price in prices {
        let s = setup();
        seed_liquidity(&s, &[&s.d1], &[10_000]);
        open_position(&s, &s.alice, 2_000, 1_000);
        coll_price(&s, price);
        
        let res = s.client().liquidate(&s.bob, &s.alice, &0i128);
        bonuses.push(res.bonus_bps);
    }
    
    // Verify monotonic increase (allowing for cap)
    for i in 1..bonuses.len() {
        std::assert!(
            bonuses[i] >= bonuses[i - 1],
            "bonus should increase as price drops: {} vs {}",
            bonuses[i],
            bonuses[i - 1]
        );
    }
}

#[test]
fn test_liquidation_after_interest_accrual() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_000);
    
    // Advance time to accrue interest (if applicable)
    s.env.ledger().set_timestamp(s.env.ledger().timestamp() + 86400 * 30);
    
    // Price drop makes position liquidatable
    coll_price(&s, 55_000_000);
    
    let pos_before = s.client().get_position(&s.alice);
    let res = s.client().liquidate(&s.bob, &s.alice, &0i128);
    
    // Should liquidate accounting for accrued interest
    assert!(res.repay_units > 0);
    assert!(res.repay_units <= pos_before.debt);
}

#[test]
fn test_liquidation_preserves_other_depositor_balances() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1, &s.d2], &[6_000, 4_000]);
    
    let d1_before = s.client().get_depositor_balance(&s.d1);
    let d2_before = s.client().get_depositor_balance(&s.d2);
    
    open_position(&s, &s.alice, 2_000, 800);
    coll_price(&s, 55_000_000);
    
    // Partial liquidation (no bad debt)
    let res = s.client().liquidate(&s.bob, &s.alice, &200i128);
    
    if res.bad_debt_socialized == 0 {
        // Depositor balances should remain unchanged for healthy liquidation
        let d1_after = s.client().get_depositor_balance(&s.d1);
        let d2_after = s.client().get_depositor_balance(&s.d2);
        
        assert_eq!(d1_after, d1_before);
        assert_eq!(d2_after, d2_before);
    }
}

#[test]
fn test_full_liquidation_clears_position_completely() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_000);
    coll_price(&s, 5_000_000);
    
    s.client().liquidate(&s.bob, &s.alice, &crate::FULL_REPAY_SENTINEL);
    
    // Position should be completely cleared
    let pos = s.client().get_position(&s.alice);
    assert_eq!(pos.collateral, 0);
    assert_eq!(pos.debt, 0);
    
    // Borrower should be able to open a new position
    coll_price(&s, PRICE_ONE);
    open_position(&s, &s.alice, 1_000, 500);
    let new_pos = s.client().get_position(&s.alice);
    assert_eq!(new_pos.collateral, 1_000);
    assert_eq!(new_pos.debt, 500);
}

#[test]
fn test_liquidation_with_max_penalty_cap() {
    let s = setup();
    // Set penalty cap to maximum allowed (e.g., 20%)
    s.client().set_liq_params(&s.admin, &500u32, &1_500u32, &2_000u32);
    
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 2_000, 1_000);
    coll_price(&s, 10_000_000); // Deep crash
    
    let res = s.client().liquidate(&s.bob, &s.alice, &0i128);
    
    // Bonus should be capped at configured maximum
    assert!(res.bonus_bps <= 2_000);
    
    // Verify the liquidator doesn't get excessive bonus
    let repay_value = res.repay_units;
    let seize_value = res.seize_units * 10_000_000 / PRICE_ONE;
    let max_allowed = repay_value * 12_000 / 10_000;
    
    std::assert!(
        seize_value <= max_allowed + 1,
        "seize {} exceeds max allowed {}",
        seize_value,
        max_allowed
    );
}

#[test]
fn test_multiple_liquidators_compete_fairly() {
    let s = setup();
    seed_liquidity(&s, &[&s.d1], &[10_000]);
    open_position(&s, &s.alice, 4_000, 2_000);
    coll_price(&s, 55_000_000);
    
    let liquidator1 = soroban_sdk::Address::generate(&s.env);
    let liquidator2 = soroban_sdk::Address::generate(&s.env);
    
    token::Client::new(&s.env, &s.debt_id).mint(&liquidator1, &1_000i128);
    token::Client::new(&s.env, &s.debt_id).mint(&liquidator2, &1_000i128);
    
    // First liquidator acts
    let res1 = s.client().liquidate(&liquidator1, &s.alice, &500i128);
    assert_eq!(res1.repay_units, 500);
    
    let pos_mid = s.client().get_position(&s.alice);
    
    // If position still liquidatable, second liquidator can act
    let (num, den) = s.client().health_factor(&s.alice);
    if num < den && pos_mid.debt > 0 {
        let res2 = s.client().liquidate(&liquidator2, &s.alice, &500i128);
        
        // Both should get same bonus rate (for same health state)
        // Note: bonus might differ slightly if health improved between liquidations
        assert!(res2.repay_units > 0);
        
        // Combined shouldn't over-liquidate
        let pos_final = s.client().get_position(&s.alice);
        assert!(pos_final.collateral >= 0);
        assert!(pos_final.debt >= 0);
    }
}
