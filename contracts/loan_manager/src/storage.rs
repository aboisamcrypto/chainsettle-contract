//! Storage layer: data types, storage keys, and typed accessors.
#![allow(dead_code)]

use crate::constants::TTL_INITIAL_LEDGERS;
use soroban_sdk::{contracttype, vec, Address, Env, Vec};

// ============================================================
// DATA TYPES
// ============================================================

/// Lending market configuration. A market pairs one collateral asset with
/// one borrowable (debt) asset.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Market {
    /// Stellar Asset Contract accepted as collateral.
    pub collateral_token: Address,
    /// Stellar Asset Contract that can be borrowed (pool liquidity asset).
    pub debt_token: Address,
    /// Liquidation threshold in basis points (e.g. 8_000 = 80%).
    /// A position is liquidatable when
    /// `collateral_value * liq_threshold_bps < debt_value * 10_000`.
    pub liq_threshold_bps: u32,
}

/// Per-user lending position.
#[contracttype]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Position {
    /// Collateral units deposited by the user.
    pub collateral: i128,
    /// Debt units owed by the user (debt token).
    pub debt: i128,
}

/// One cumulative-price snapshot used for TWAP computation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceObservation {
    /// Ledger timestamp (seconds) at which the snapshot was recorded.
    pub ts: u64,
    /// Cumulative price-seconds accumulator value at `ts`.
    pub cum: i128,
}

/// State of the time-weighted average price accumulator for one asset.
#[contracttype]
#[derive(Clone, Debug)]
pub struct TwapState {
    /// Timestamp of the last observed price (0 = not seeded).
    pub last_ts: u64,
    /// Last directly observed price.
    pub last_price: i128,
    /// Cumulative sum of price * elapsed seconds.
    pub cumulative: i128,
    /// Timestamp of the most recent snapshot.
    pub last_snapshot_ts: u64,
    /// Ring buffer of snapshots (bounded, oldest first).
    pub snapshots: Vec<PriceObservation>,
}

impl TwapState {
    pub fn empty(env: &Env) -> TwapState {
        TwapState {
            last_ts: 0,
            last_price: 0,
            cumulative: 0,
            last_snapshot_ts: 0,
            snapshots: vec![env],
        }
    }
}

/// Admin-configurable liquidation parameters.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct LiqParams {
    /// Bonus at health factor = 1.0, in bps (default 500 = 5%).
    pub base_bonus_bps: u32,
    /// Maximum dynamic bonus, in bps (default 1500 = 15%).
    pub max_bonus_bps: u32,
    /// Hard cap on the penalty, in bps (default 2000 = 20%).
    pub penalty_cap_bps: u32,
}

impl Default for LiqParams {
    fn default() -> Self {
        use crate::constants::{
            BASE_LIQUIDATION_BONUS_BPS, LIQUIDATION_PENALTY_CAP_BPS, MAX_DYNAMIC_BONUS_BPS,
        };
        LiqParams {
            base_bonus_bps: BASE_LIQUIDATION_BONUS_BPS,
            max_bonus_bps: MAX_DYNAMIC_BONUS_BPS,
            penalty_cap_bps: LIQUIDATION_PENALTY_CAP_BPS,
        }
    }
}

/// Result of a price resolution through the failover chain.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceResolution {
    pub price: i128,
    /// "primary", "secondary" or "twap".
    pub source: soroban_sdk::Symbol,
}

// ============================================================
// STORAGE KEYS
// ============================================================

#[contracttype]
pub enum DataKey {
    Admin,
    Paused,
    Market,
    PrimaryOracle,
    SecondaryOracle,
    /// Per-user lending position.
    Position(Address),
    /// Liquidity-provider balance in the debt token.
    DepositorBalance(Address),
    /// Ordered list of depositor addresses (for pro-rata socialization).
    Depositors,
    /// Total liquidity owed to depositors (debt token units).
    TotalDeposits,
    /// Total outstanding borrower debt (debt token units).
    TotalBorrowed,
    /// Per-asset TWAP accumulator state.
    Twap(Address),
    /// Admin-configurable liquidation parameters.
    LiqParams,
    /// Count of times the failover chain fell through to TWAP (diagnostics).
    TwapFallbackCount,
}

// ============================================================
// RAW STORAGE HELPERS
// ============================================================

/// Store `val` under a persistent key and refresh its TTL.
pub fn set<T: Clone + soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(
    env: &Env,
    key: DataKey,
    val: &T,
) {
    env.storage().persistent().set(&key, val);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_INITIAL_LEDGERS, TTL_INITIAL_LEDGERS);
}

/// Load an optional value from persistent storage.
pub fn get_opt<T: Clone + soroban_sdk::TryFromVal<soroban_sdk::Env, soroban_sdk::Val>>(
    env: &Env,
    key: &DataKey,
) -> Option<T> {
    env.storage().persistent().get(key)
}

/// Remove a key from persistent storage.
pub fn remove(env: &Env, key: DataKey) {
    env.storage().persistent().remove(&key);
}

// ============================================================
// TYPED ACCESSORS
// ============================================================

pub fn get_admin(env: &Env) -> Option<Address> {
    get_opt(env, &DataKey::Admin)
}

pub fn require_admin(env: &Env) -> Address {
    get_admin(env).unwrap_or_else(|| panic!("admin not set"))
}

pub fn is_paused(env: &Env) -> bool {
    get_opt(env, &DataKey::Paused).unwrap_or(false)
}

pub fn set_paused(env: &Env, paused: bool) {
    set(env, DataKey::Paused, &paused);
}

pub fn get_market(env: &Env) -> Option<Market> {
    get_opt(env, &DataKey::Market)
}

pub fn require_market(env: &Env) -> Market {
    get_market(env).unwrap_or_else(|| panic!("market not initialized"))
}

pub fn get_primary_oracle(env: &Env) -> Option<Address> {
    get_opt(env, &DataKey::PrimaryOracle)
}

pub fn get_secondary_oracle(env: &Env) -> Option<Address> {
    get_opt(env, &DataKey::SecondaryOracle)
}

pub fn get_liq_params(env: &Env) -> LiqParams {
    get_opt(env, &DataKey::LiqParams).unwrap_or_default()
}

pub fn get_position(env: &Env, user: &Address) -> Position {
    get_opt(env, &DataKey::Position(user.clone())).unwrap_or_default()
}

pub fn save_position(env: &Env, user: &Address, pos: &Position) {
    set(env, DataKey::Position(user.clone()), pos);
}

pub fn get_depositor_balance(env: &Env, depositor: &Address) -> i128 {
    get_opt(env, &DataKey::DepositorBalance(depositor.clone())).unwrap_or(0)
}

pub fn get_depositors(env: &Env) -> Vec<Address> {
    get_opt(env, &DataKey::Depositors).unwrap_or_else(|| vec![env])
}

pub fn register_depositor(env: &Env, depositor: &Address) {
    let mut list = get_depositors(env);
    if !list.contains(depositor) {
        list.push_back(depositor.clone());
        set(env, DataKey::Depositors, &list);
    }
}

pub fn get_total_deposits(env: &Env) -> i128 {
    get_opt(env, &DataKey::TotalDeposits).unwrap_or(0)
}

pub fn get_total_borrowed(env: &Env) -> i128 {
    get_opt(env, &DataKey::TotalBorrowed).unwrap_or(0)
}

pub fn get_twap_state(env: &Env, asset: &Address) -> TwapState {
    get_opt(env, &DataKey::Twap(asset.clone())).unwrap_or_else(|| TwapState::empty(env))
}

pub fn bump_twap_fallback_count(env: &Env) -> u32 {
    let n: u32 = get_opt(env, &DataKey::TwapFallbackCount).unwrap_or(0);
    set(env, DataKey::TwapFallbackCount, &(n + 1));
    n + 1
}
