//! Liquidation engine: pure math core plus plan computation.
//!
//! Design goals (see README/PRD):
//! - Dynamic liquidator incentives: bonus interpolates linearly from
//!   `base_bonus` at HF = 1.0 up to `max_bonus` at HF <= 0.5.
//! - Penalty caps: the effective bonus never exceeds `penalty_cap_bps`,
//!   i.e. the incentive portion can never exceed 20% of the seized
//!   collateral value by default.
//! - Partial liquidation: by default liquidate only enough debt to restore
//!   the position's health factor to 1.1, instead of draining it whole.
//! - Bad-debt socialization: when a liquidation drains all collateral and
//!   debt still remains, the shortfall is written off proportionally across
//!   all liquidity depositors.
#![allow(dead_code)]

use soroban_sdk::{Env, Vec};

use crate::constants::{BPS_DENOMINATOR, PRICE_SCALE, TARGET_HF_DEN, TARGET_HF_NUM};

// ============================================================
// HEALTH FACTOR
// ============================================================

/// Health ratio expressed as an exact fraction `(num, den)`:
/// `HF = num / den` where
/// `num = collateral_value * liq_threshold_bps` and
/// `den = debt_value * 10_000`.
///
/// Comparing `num < den` avoids any fixed-point materialization and cannot
/// overflow for realistic magnitudes (values are USD-scaled at 1e8).
pub fn health_ratio(coll_value: i128, debt_value: i128, liq_threshold_bps: u32) -> (i128, i128) {
    let num = coll_value.saturating_mul(liq_threshold_bps as i128);
    let den = debt_value.saturating_mul(BPS_DENOMINATOR);
    (num, den)
}

/// A position is liquidatable when its health factor drops below 1.
pub fn is_liquidatable(num: i128, den: i128) -> bool {
    den > 0 && num < den
}

// ============================================================
// DYNAMIC LIQUIDATOR INCENTIVES
// ============================================================

/// Effective liquidation bonus in bps.
///
/// Linear interpolation from `base_bonus_bps` at HF = 1.0 to
/// `max_bonus_bps` at HF <= 0.5, then hard-capped at `penalty_cap_bps`.
/// The cap guarantees the bonus portion of a seizure can never exceed
/// `penalty_cap_bps / 100` percent of the collateral value.
pub fn dynamic_bonus_bps(
    num: i128,
    den: i128,
    base_bonus_bps: u32,
    max_bonus_bps: u32,
    penalty_cap_bps: u32,
) -> u32 {
    debug_assert!(base_bonus_bps <= max_bonus_bps);
    let base = base_bonus_bps as i128;
    let span = (max_bonus_bps.saturating_sub(base_bonus_bps)) as i128;

    let bonus = if den <= 0 || num >= den || span == 0 {
        // Healthy (or degenerate) positions get the base bonus; callers
        // must have verified liquidatability beforehand anyway.
        base
    } else {
        // depth = 1 - HF, reaching 0.5 at HF = 0.5.
        let depth_x2 = ((den - num).saturating_mul(2)).min(den.saturating_mul(2));
        base + span.saturating_mul(depth_x2) / den
    };

    bonus.clamp(base, base + span).min(penalty_cap_bps as i128) as u32
}

// ============================================================
// VALUE / UNIT CONVERSIONS
// ============================================================

/// Convert token units to USD value using an oracle price.
pub fn units_to_value(units: i128, price: i128) -> i128 {
    if price <= 0 {
        return 0;
    }
    units.saturating_mul(price) / PRICE_SCALE
}

/// Convert USD value back to token units (floor).
pub fn value_to_units(value: i128, price: i128) -> i128 {
    if price <= 0 {
        return 0;
    }
    value.saturating_mul(PRICE_SCALE) / price
}

// ============================================================
// LIQUIDATION PLAN
// ============================================================

/// How much debt the caller asks to repay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RepayMode {
    /// Repay exactly enough to bring the health factor back to the
    /// partial-liquidation target (1.1 by default). Recommended default.
    PartialToTarget,
    /// Repay the entire remaining debt (drains the position).
    Full,
    /// Caller-specified debt-token units; always capped by the engine rules.
    Specified(i128),
}

/// Outcome of the liquidation math for one call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidationPlan {
    /// Debt-token units the liquidator must pay into the pool.
    pub repay_units: i128,
    /// Collateral units transferred to the liquidator.
    pub seize_units: i128,
    /// Effective bonus applied, in bps (already penalty-cap bounded).
    pub bonus_bps: u32,
    /// Remaining borrower debt (debt-token units) written off through
    /// bad-debt socialization. Zero when the position stays alive.
    pub bad_debt_units: i128,
}

/// Compute the liquidation plan for a position.
///
/// Guarantees:
/// - `seize_units <= coll_units`
/// - `bonus_bps <= penalty_cap_bps` (liquidation penalty cap)
/// - `repay_units <= debt_units`
/// - when the seizure does NOT drain all collateral, the resulting health
///   factor is >= the partial-liquidation target
/// - when the seizure DOES drain all collateral and debt remains, the
///   remainder is returned as `bad_debt_units` for socialization
#[allow(clippy::too_many_arguments)]
pub fn compute_plan(
    coll_units: i128,
    debt_units: i128,
    price_coll: i128,
    price_debt: i128,
    liq_threshold_bps: u32,
    base_bonus_bps: u32,
    max_bonus_bps: u32,
    penalty_cap_bps: u32,
    mode: RepayMode,
) -> LiquidationPlan {
    let coll_value = units_to_value(coll_units, price_coll);
    let debt_value = units_to_value(debt_units, price_debt);
    let (num, den) = health_ratio(coll_value, debt_value, liq_threshold_bps);

    let empty = LiquidationPlan {
        repay_units: 0,
        seize_units: 0,
        bonus_bps: dynamic_bonus_bps(num, den, base_bonus_bps, max_bonus_bps, penalty_cap_bps),
        bad_debt_units: 0,
    };

    if !is_liquidatable(num, den) {
        return empty;
    }

    let bonus_bps = dynamic_bonus_bps(num, den, base_bonus_bps, max_bonus_bps, penalty_cap_bps);

    // --- Target repayment in value terms -----------------------------
    // Solve for r such that the post-liquidation health factor equals the
    // target T (= 1.1), accounting for BOTH sides of the ratio:
    //
    //   (C - r*g/BPS) * thr      T = NUM/DEN
    //   --------------------- = ----
    //      (D - r) * BPS
    //
    //   =>  r = BPS * (NUM*BPS*D - DEN*thr*C) / (NUM*BPS^2 - DEN*g*thr)
    //
    // A non-positive denominator means the target is mathematically
    // unreachable (deeply underwater position); we then fall back to
    // maximum repayment and let the affordability cap decide.
    let n_bs = TARGET_HF_NUM.saturating_mul(BPS_DENOMINATOR); // NUM*BPS
    let thr = liq_threshold_bps as i128;
    let gross = BPS_DENOMINATOR.saturating_add(bonus_bps as i128);
    let denom = n_bs
        .saturating_mul(BPS_DENOMINATOR)
        .saturating_sub(TARGET_HF_DEN.saturating_mul(gross).saturating_mul(thr));

    let target_value = match mode {
        RepayMode::PartialToTarget => {
            if denom > 0 {
                let inner = n_bs
                    .saturating_mul(debt_value)
                    .saturating_sub(TARGET_HF_DEN.saturating_mul(thr).saturating_mul(coll_value));
                (BPS_DENOMINATOR.saturating_mul(inner.max(0)) / denom).max(0)
            } else {
                // Unreachable target: repay as much as possible.
                debt_value
            }
        }
        RepayMode::Full => debt_value,
        RepayMode::Specified(units) => units_to_value(units.max(0), price_debt),
    };

    // Cap by the requested amount itself (specified mode) and by total debt.
    let mut repay_value = target_value.min(debt_value).max(0);

    // Collateral affordability cap:
    //   seize_value = repay_value * (BPS + bonus) / BPS  <= coll_value
    // Rounded UP so that fully-drained positions are never left with a
    // dust sliver of collateral due to integer flooring.
    let affordable = (coll_value
        .saturating_mul(BPS_DENOMINATOR)
        .saturating_add(gross - 1))
        / gross;
    repay_value = repay_value.min(affordable).max(0);

    let repay_units = value_to_units(repay_value, price_debt)
        .min(debt_units)
        .max(0);
    if repay_units == 0 {
        // Nothing economically repayable (e.g. zero-value collateral):
        // the whole remaining debt is bad debt.
        return LiquidationPlan {
            repay_units: 0,
            seize_units: 0,
            bonus_bps,
            bad_debt_units: debt_units,
        };
    }

    // Seizure includes the (capped) bonus. Flooring keeps the liquidator
    // from ever receiving more than they are entitled to.
    let seize_value = repay_value.saturating_mul(gross) / BPS_DENOMINATOR;
    let seize_units = value_to_units(seize_value, price_coll)
        .min(coll_units)
        .max(0);

    // A liquidation "drains" the position when the seized value covers the
    // entire collateral value — the remaining debt is then bad debt to be
    // socialized rather than an obligation the collateral can still back.
    let drained = coll_value > 0 && seize_value >= coll_value;
    let remaining_debt = debt_units - repay_units;
    let bad_debt_units = if drained { remaining_debt } else { 0 };

    LiquidationPlan {
        repay_units,
        seize_units,
        bonus_bps,
        bad_debt_units,
    }
}

// ============================================================
// BAD-DEBT SOCIALIZATION MATH
// ============================================================

/// Proportional loss shares for each depositor balance (floor division).
/// Returns deductions aligned with the input order. The sum of returned
/// shares may be slightly below `loss` due to rounding; callers apply the
/// residual to solvent accounts afterwards.
pub fn proportional_shares(env: &Env, balances: &Vec<i128>, total: i128, loss: i128) -> Vec<i128> {
    let mut out = Vec::new(env);
    if total <= 0 || loss <= 0 {
        for _ in 0..balances.len() {
            out.push_back(0);
        }
        return out;
    }
    for b in balances.iter() {
        out.push_back(b.saturating_mul(loss) / total);
    }
    out
}
