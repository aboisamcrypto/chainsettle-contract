//! Loan Manager — Soroban lending pool with a hardened liquidation engine.
//!
//! Features (PRD):
//! - Oracle failover chain: primary → secondary → 30-minute TWAP
//!   ([`oracle::resolve_price`]).
//! - Bad-debt socialization: shortfalls after full collateral drain are
//!   distributed pro-rata across liquidity depositors.
//! - Dynamic liquidator incentives: 5%–15% bonus scaled by position
//!   health, hard-capped at 20% of collateral value.
//! - Partial liquidation: default mode restores the health factor to 1.1
//!   instead of draining the whole position.
#![no_std]

#[cfg(any(test, feature = "testutils"))]
extern crate std;

mod constants;
mod liquidation;
mod oracle;
mod storage;

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, Symbol, Vec};

use crate::constants::{MAX_CONFIGURABLE_BONUS_BPS, MAX_LIQ_THRESHOLD_BPS};
use crate::liquidation::{compute_plan, proportional_shares, RepayMode};
use crate::oracle::{resolve_price, twap_price};
use crate::storage::{
    get_depositor_balance, get_depositors, get_liq_params, get_position, get_primary_oracle,
    get_secondary_oracle, get_total_borrowed, get_total_deposits, get_twap_state, is_paused,
    register_depositor, remove, require_admin, require_market, save_position, set, set_paused,
    DataKey, LiqParams, Market, Position, PriceResolution,
};

/// Sentinel repay amount for full-liquidation mode.
pub const FULL_REPAY_SENTINEL: i128 = i128::MAX;

// ============================================================
// CONTRACT
// ============================================================

#[contract]
pub struct LoanManagerContract;

/// Public liquidation outcome, returned for off-chain indexing.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct LiquidationResult {
    pub repay_units: i128,
    pub seize_units: i128,
    pub bonus_bps: u32,
    pub bad_debt_socialized: i128,
}

fn require_not_paused(env: &Env) {
    if is_paused(env) {
        panic!("contract is paused");
    }
}

fn debt_token_client(env: &Env) -> token::Client<'_> {
    let market = require_market(env);
    token::Client::new(env, &market.debt_token)
}

fn collateral_token_client(env: &Env) -> token::Client<'_> {
    let market = require_market(env);
    token::Client::new(env, &market.collateral_token)
}

fn contract_address(env: &Env) -> Address {
    env.current_contract_address()
}

/// Ensure the signer matches the stored admin (defense in depth on top of
/// `require_auth`).
fn assert_admin(env: &Env, admin: &Address) {
    let stored = require_admin(env);
    if stored != *admin {
        panic!("unauthorized");
    }
}

/// Credit `amount` debt-token units back to the pool accounting after
/// someone pays debt into the contract.
fn reduce_borrowed(env: &Env, amount: i128) {
    let tb = get_total_borrowed(env);
    set(env, DataKey::TotalBorrowed, &(tb.saturating_sub(amount)));
}

/// Write off `loss` units of bad debt across all liquidity depositors,
/// proportionally to their balances. Rounding residuals are taken from any
/// account with remaining capacity so the write-off always covers `loss`
/// whenever the depositor base can absorb it.
fn socialize_bad_debt(env: &Env, borrower: &Address, loss: i128) -> i128 {
    if loss <= 0 {
        return 0;
    }
    let depositors = get_depositors(env);
    let total_deposits = get_total_deposits(env);
    if total_deposits <= 0 || depositors.is_empty() {
        // Nothing to socialize against; the pool simply absorbs the hole.
        set(env, DataKey::TotalDeposits, &0i128);
        env.events().publish(
            (Symbol::new(env, "bad_debt_uncollateralized"),),
            (borrower.clone(), loss),
        );
        return 0;
    }

    let mut balances = soroban_sdk::Vec::new(env);
    for d in depositors.iter() {
        balances.push_back(get_depositor_balance(env, &d));
    }
    let shares = proportional_shares(env, &balances, total_deposits, loss);

    let mut applied_total: i128 = 0;
    let mut residual = loss;
    // Pass 1: proportional deductions (floored at each balance).
    for (i, d) in depositors.iter().enumerate() {
        let bal = balances.get(i as u32).unwrap_or(0);
        let mut cut = shares.get(i as u32).unwrap_or(0).min(bal);
        cut = cut.min(residual);
        if cut > 0 {
            set(env, DataKey::DepositorBalance(d.clone()), &(bal - cut));
            applied_total += cut;
            residual -= cut;
        }
    }
    // Pass 2: rounding residual from accounts with spare capacity.
    if residual > 0 {
        for d in depositors.iter() {
            if residual == 0 {
                break;
            }
            let bal = get_depositor_balance(env, &d);
            let spare = bal.min(residual);
            if spare > 0 {
                set(env, DataKey::DepositorBalance(d.clone()), &(bal - spare));
                applied_total += spare;
                residual -= spare;
            }
        }
    }

    let td = get_total_deposits(env);
    set(
        env,
        DataKey::TotalDeposits,
        &(td.saturating_sub(applied_total)),
    );

    env.events().publish(
        (Symbol::new(env, "bad_debt_socialized"),),
        (borrower.clone(), applied_total, depositors.len()),
    );

    applied_total
}

// ============================================================
// CONTRACT IMPL
// ============================================================

#[contractimpl]
impl LoanManagerContract {
    // --------------------------------------------------------
    // ADMIN & CONFIGURATION
    // --------------------------------------------------------

    /// Initialise the contract with an administrator.
    pub fn init(env: Env, admin: Address) {
        admin.require_auth();
        if storage::get_admin(&env).is_some() {
            panic!("already initialized");
        }
        set(&env, DataKey::Admin, &admin);
        set(&env, DataKey::LiqParams, &LiqParams::default());
        env.events()
            .publish((Symbol::new(&env, "initialized"),), admin.clone());
    }

    /// Configure the oracle failover chain (primary first, then secondary).
    pub fn set_oracles(
        env: Env,
        admin: Address,
        primary: Option<Address>,
        secondary: Option<Address>,
    ) {
        admin.require_auth();
        assert_admin(&env, &admin);
        if let Some(p) = &primary {
            set(&env, DataKey::PrimaryOracle, p);
        } else {
            remove(&env, DataKey::PrimaryOracle);
        }
        if let Some(s) = &secondary {
            set(&env, DataKey::SecondaryOracle, s);
        } else {
            remove(&env, DataKey::SecondaryOracle);
        }
        env.events().publish(
            (Symbol::new(&env, "oracles_set"),),
            (primary.clone(), secondary.clone()),
        );
    }

    /// Configure the single lending market (collateral/debt pair).
    pub fn set_market(
        env: Env,
        admin: Address,
        collateral_token: Address,
        debt_token: Address,
        liq_threshold_bps: u32,
    ) {
        admin.require_auth();
        assert_admin(&env, &admin);
        if liq_threshold_bps == 0 || liq_threshold_bps > MAX_LIQ_THRESHOLD_BPS {
            panic!("invalid liquidation threshold");
        }
        if collateral_token == debt_token {
            panic!("collateral and debt tokens must differ");
        }
        let market = Market {
            collateral_token,
            debt_token,
            liq_threshold_bps,
        };
        set(&env, DataKey::Market, &market);
        env.events()
            .publish((Symbol::new(&env, "market_set"),), market);
    }

    /// Tune liquidation incentives. All bps values are validated and the
    /// penalty cap is always enforced downstream regardless of inputs.
    pub fn set_liq_params(
        env: Env,
        admin: Address,
        base_bonus_bps: u32,
        max_bonus_bps: u32,
        penalty_cap_bps: u32,
    ) {
        admin.require_auth();
        assert_admin(&env, &admin);
        if base_bonus_bps > MAX_CONFIGURABLE_BONUS_BPS
            || max_bonus_bps > MAX_CONFIGURABLE_BONUS_BPS
            || penalty_cap_bps > MAX_CONFIGURABLE_BONUS_BPS
        {
            panic!("bonus parameters exceed maximum");
        }
        if base_bonus_bps > max_bonus_bps {
            panic!("base bonus cannot exceed max bonus");
        }
        let params = LiqParams {
            base_bonus_bps,
            max_bonus_bps,
            penalty_cap_bps,
        };
        set(&env, DataKey::LiqParams, &params);
        env.events()
            .publish((Symbol::new(&env, "liq_params_set"),), params);
    }

    /// Emergency circuit breaker: halt all state-changing operations.
    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        assert_admin(&env, &admin);
        set_paused(&env, true);
        env.events().publish((Symbol::new(&env, "paused"),), admin);
    }

    /// Resume normal operation.
    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        assert_admin(&env, &admin);
        set_paused(&env, false);
        env.events()
            .publish((Symbol::new(&env, "unpaused"),), admin);
    }

    // --------------------------------------------------------
    // LIQUIDITY (DEBT-TOKEN DEPOSITORS)
    // --------------------------------------------------------

    /// Supply debt-token liquidity to the pool. Earns nothing by itself;
    /// exposed to pro-rata bad-debt socialization.
    pub fn deposit(env: Env, depositor: Address, amount: i128) {
        depositor.require_auth();
        require_not_paused(&env);
        require_market(&env);
        if amount <= 0 {
            panic!("invalid amount");
        }
        debt_token_client(&env).transfer(&depositor, &contract_address(&env), &amount);

        let bal = get_depositor_balance(&env, &depositor);
        set(
            &env,
            DataKey::DepositorBalance(depositor.clone()),
            &(bal + amount),
        );
        register_depositor(&env, &depositor);
        let td = get_total_deposits(&env);
        set(&env, DataKey::TotalDeposits, &(td + amount));

        env.events().publish(
            (Symbol::new(&env, "liquidity_deposited"),),
            (depositor, amount),
        );
    }

    /// Withdraw previously supplied liquidity. Blocked beyond free pool
    /// liquidity (`total_deposits - total_borrowed`).
    pub fn withdraw(env: Env, depositor: Address, amount: i128) {
        depositor.require_auth();
        require_not_paused(&env);
        require_market(&env);
        if amount <= 0 {
            panic!("invalid amount");
        }
        let bal = get_depositor_balance(&env, &depositor);
        if bal < amount {
            panic!("insufficient depositor balance");
        }
        let free = get_total_deposits(&env).saturating_sub(get_total_borrowed(&env));
        if free < amount {
            panic!("insufficient pool liquidity");
        }
        debt_token_client(&env).transfer(&contract_address(&env), &depositor, &amount);

        set(
            &env,
            DataKey::DepositorBalance(depositor.clone()),
            &(bal - amount),
        );
        let td = get_total_deposits(&env);
        set(&env, DataKey::TotalDeposits, &(td - amount));

        env.events().publish(
            (Symbol::new(&env, "liquidity_withdrawn"),),
            (depositor, amount),
        );
    }

    // --------------------------------------------------------
    // BORROWER POSITIONS
    // --------------------------------------------------------

    /// Deposit collateral tokens backing future borrows.
    pub fn deposit_collateral(env: Env, user: Address, amount: i128) {
        user.require_auth();
        require_not_paused(&env);
        require_market(&env);
        if amount <= 0 {
            panic!("invalid amount");
        }
        collateral_token_client(&env).transfer(&user, &contract_address(&env), &amount);

        let mut pos = get_position(&env, &user);
        pos.collateral += amount;
        save_position(&env, &user, &pos);

        env.events()
            .publish((Symbol::new(&env, "collateral_added"),), (user, amount));
    }

    /// Borrow debt-token units against posted collateral. Reverts when the
    /// resulting health factor would drop below 1.
    pub fn borrow(env: Env, user: Address, amount: i128) {
        user.require_auth();
        require_not_paused(&env);
        let market = require_market(&env);
        if amount <= 0 {
            panic!("invalid amount");
        }
        let free = get_total_deposits(&env).saturating_sub(get_total_borrowed(&env));
        if free < amount {
            panic!("insufficient pool liquidity");
        }

        let mut pos = get_position(&env, &user);
        let price_coll = oracle::require_price(&env, &market.collateral_token);
        let price_debt = oracle::require_price(&env, &market.debt_token);

        let coll_value = liquidation::units_to_value(pos.collateral, price_coll);
        let new_debt_value =
            liquidation::units_to_value(pos.debt.saturating_add(amount), price_debt);
        let (num, den) =
            liquidation::health_ratio(coll_value, new_debt_value, market.liq_threshold_bps);
        if !(num >= den) {
            panic!("insufficient collateral");
        }

        debt_token_client(&env).transfer(&contract_address(&env), &user, &amount);
        pos.debt += amount;
        save_position(&env, &user, &pos);
        let tb = get_total_borrowed(&env);
        set(&env, DataKey::TotalBorrowed, &(tb + amount));

        env.events()
            .publish((Symbol::new(&env, "borrowed"),), (user, amount));
    }

    /// Repay borrowed units (capped at the outstanding debt).
    pub fn repay(env: Env, user: Address, amount: i128) {
        user.require_auth();
        require_not_paused(&env);
        require_market(&env);
        if amount <= 0 {
            panic!("invalid amount");
        }
        let mut pos = get_position(&env, &user);
        let amount = amount.min(pos.debt);
        if amount == 0 {
            panic!("nothing to repay");
        }
        debt_token_client(&env).transfer(&user, &contract_address(&env), &amount);

        pos.debt -= amount;
        save_position(&env, &user, &pos);
        reduce_borrowed(&env, amount);

        env.events()
            .publish((Symbol::new(&env, "repaid"),), (user, amount));
    }

    /// Withdraw collateral while keeping the position healthy.
    pub fn withdraw_collateral(env: Env, user: Address, amount: i128) {
        user.require_auth();
        require_not_paused(&env);
        let market = require_market(&env);
        if amount <= 0 {
            panic!("invalid amount");
        }
        let mut pos = get_position(&env, &user);
        if pos.collateral < amount {
            panic!("insufficient collateral");
        }

        if pos.debt > 0 {
            let price_coll = oracle::require_price(&env, &market.collateral_token);
            let price_debt = oracle::require_price(&env, &market.debt_token);
            let coll_value = liquidation::units_to_value(pos.collateral - amount, price_coll);
            let debt_value = liquidation::units_to_value(pos.debt, price_debt);
            let (num, den) =
                liquidation::health_ratio(coll_value, debt_value, market.liq_threshold_bps);
            if !(num >= den) {
                panic!("withdrawal would undercollateralize position");
            }
        }

        collateral_token_client(&env).transfer(&contract_address(&env), &user, &amount);
        pos.collateral -= amount;
        save_position(&env, &user, &pos);

        env.events()
            .publish((Symbol::new(&env, "collateral_withdrawn"),), (user, amount));
    }

    // --------------------------------------------------------
    // LIQUIDATION ENGINE
    // --------------------------------------------------------
    //
    // `repay_amount` semantics:
    //   * `FULL_REPAY_SENTINEL` (i128::MAX) → full liquidation: repay the
    //     entire remaining debt and drain the position.
    //   * `0` → partial liquidation: repay only enough to restore the
    //     health factor to the 1.1 target.
    //   * `n > 0` → caller-specified repayment, always capped by the
    //     engine's safety rules.

    /// Execute a liquidation against `borrower`'s position. Open to any
    /// account (permissionless). Applies dynamic incentives within the
    /// penalty cap and triggers bad-debt socialization when the seizure
    /// drains all collateral with debt remaining.
    pub fn liquidate(
        env: Env,
        liquidator: Address,
        borrower: Address,
        repay_amount: i128,
    ) -> LiquidationResult {
        liquidator.require_auth();
        require_not_paused(&env);
        let market = require_market(&env);
        if liquidator == borrower {
            panic!("cannot self-liquidate");
        }

        let pos = get_position(&env, &borrower);
        if pos.debt <= 0 {
            panic!("position not liquidatable");
        }

        // Resolve prices through the failover chain (also feeds TWAP history).
        let res_coll = resolve_price(&env, &market.collateral_token);
        let res_debt = resolve_price(&env, &market.debt_token);

        let params = get_liq_params(&env);
        let mode = if repay_amount == FULL_REPAY_SENTINEL {
            RepayMode::Full
        } else if repay_amount == 0 {
            RepayMode::PartialToTarget
        } else {
            if repay_amount < 0 {
                panic!("invalid amount");
            }
            RepayMode::Specified(repay_amount)
        };

        let plan = compute_plan(
            pos.collateral,
            pos.debt,
            res_coll.price,
            res_debt.price,
            market.liq_threshold_bps,
            params.base_bonus_bps,
            params.max_bonus_bps,
            params.penalty_cap_bps,
            mode,
        );

        if plan.repay_units == 0 && plan.bad_debt_units == 0 {
            panic!("position not liquidatable");
        }

        // Move funds: liquidator repays debt-token into the pool…
        if plan.repay_units > 0 {
            debt_token_client(&env).transfer(
                &liquidator,
                &contract_address(&env),
                &plan.repay_units,
            );
        }
        // …and receives collateral (+ capped bonus).
        if plan.seize_units > 0 {
            collateral_token_client(&env).transfer(
                &contract_address(&env),
                &liquidator,
                &plan.seize_units,
            );
        }

        // Apply state.
        let mut updated = pos.clone();
        updated.collateral -= plan.seize_units;
        updated.debt = updated
            .debt
            .saturating_sub(plan.repay_units)
            .saturating_sub(plan.bad_debt_units);
        reduce_borrowed(&env, plan.repay_units + plan.bad_debt_units);

        // Bad-debt socialization when the position was drained with debt left.
        let socialized = if plan.bad_debt_units > 0 {
            socialize_bad_debt(&env, &borrower, plan.bad_debt_units)
        } else {
            0
        };

        if updated.collateral == 0 && updated.debt == 0 {
            remove(&env, DataKey::Position(borrower.clone()));
        } else {
            save_position(&env, &borrower, &updated);
        }

        env.events().publish(
            (Symbol::new(&env, "liquidated"),),
            (
                borrower.clone(),
                liquidator.clone(),
                plan.repay_units,
                plan.seize_units,
                plan.bonus_bps,
            ),
        );

        LiquidationResult {
            repay_units: plan.repay_units,
            seize_units: plan.seize_units,
            bonus_bps: plan.bonus_bps,
            bad_debt_socialized: socialized,
        }
    }

    // --------------------------------------------------------
    // KEEPER & VIEW FUNCTIONS
    // --------------------------------------------------------

    /// Keeper hook: refresh the TWAP accumulator from the live failover
    /// chain without performing any other action.
    pub fn record_price(env: Env, asset: Address) {
        require_not_paused(&env);
        oracle::resolve_price(&env, &asset);
    }

    /// Current position of `user`.
    pub fn get_position(env: Env, user: Address) -> Position {
        get_position(&env, &user)
    }

    /// Liquidity balance recorded for `depositor`.
    pub fn get_depositor_balance(env: Env, depositor: Address) -> i128 {
        storage::get_depositor_balance(&env, &depositor)
    }

    /// Health factor of `user` as the exact fraction `(num, den)`;
    /// healthy iff `num >= den`.
    pub fn health_factor(env: Env, user: Address) -> (i128, i128) {
        let market = require_market(&env);
        let pos = get_position(&env, &user);
        if pos.debt == 0 {
            return (i128::MAX, 1);
        }
        let price_coll = oracle::require_price(&env, &market.collateral_token);
        let price_debt = oracle::require_price(&env, &market.debt_token);
        let coll_value = liquidation::units_to_value(pos.collateral, price_coll);
        let debt_value = liquidation::units_to_value(pos.debt, price_debt);
        liquidation::health_ratio(coll_value, debt_value, market.liq_threshold_bps)
    }

    /// Preview the price resolution path for `asset` (diagnostics).
    pub fn preview_price(env: Env, asset: Address) -> PriceResolution {
        oracle::resolve_price(&env, &asset)
    }

    /// Stored TWAP accumulator state for `asset`.
    pub fn get_twap(env: Env, asset: Address) -> soroban_sdk::Vec<storage::PriceObservation> {
        get_twap_state(&env, &asset).snapshots
    }

    /// Pure TWAP computation over the trailing window (diagnostics).
    pub fn twap_now(env: Env, asset: Address) -> i128 {
        twap_price(&env, &asset, env.ledger().timestamp())
    }

    /// Pool-level accounting snapshot.
    pub fn get_pool_info(env: Env) -> (i128, i128, u32) {
        (
            get_total_deposits(&env),
            get_total_borrowed(&env),
            storage::get_opt::<u32>(&env, &DataKey::TwapFallbackCount).unwrap_or(0),
        )
    }

    /// Depositor registry (ordered).
    pub fn get_depositors(env: Env) -> Vec<Address> {
        get_depositors(&env)
    }

    /// Effective liquidation parameters.
    pub fn get_liquidation_params(env: Env) -> LiqParams {
        get_liq_params(&env)
    }

    /// Market configuration.
    pub fn get_market_config(env: Env) -> Market {
        require_market(&env)
    }

    /// Oracle chain configuration.
    pub fn get_oracles(env: Env) -> Option<(Address, Option<Address>)> {
        get_primary_oracle(&env).map(|p| (p, get_secondary_oracle(&env)))
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        storage::get_admin(&env)
    }

    pub fn is_paused_view(env: Env) -> bool {
        is_paused(&env)
    }
}

// Silence unused-import warnings for re-exported helpers used in macros.
#[cfg(test)]
mod test;
