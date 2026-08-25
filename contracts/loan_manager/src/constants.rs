//! Contract-wide constants for the loan manager liquidation engine.

/// Minimum TTL ledgers for persistent storage entries (~1 day at 5s/ledger).
pub const TTL_INITIAL_LEDGERS: u32 = 100_000;

/// Oracle price scale. Prices are expressed in USD with 8 decimals,
/// i.e. a price of `50_000_000` means 0.5 USD per token unit.
pub const PRICE_SCALE: i128 = 100_000_000;

/// Basis-point denominator (10_000 bps = 100%).
pub const BPS_DENOMINATOR: i128 = 10_000;

// ------------------------------------------------------------
// ORACLE FAILOVER
// ------------------------------------------------------------

/// Maximum age of a direct oracle answer before it is considered stale
/// and skipped in the failover chain (5 minutes).
pub const MAX_PRICE_STALENESS_SECS: u64 = 300;

/// Tolerated future-timestamp skew from oracles (1 minute).
pub const MAX_FUTURE_SKEW_SECS: u64 = 60;

/// Time-weighted average price window: 30 minutes.
pub const TWAP_WINDOW_SECS: u64 = 1_800;

/// Minimum observation span required before the TWAP fallback may be used
/// (5 minutes). Prevents short-window averages from being gamed.
pub const TWAP_MIN_SPAN_SECS: u64 = 300;

/// Cadence at which cumulative-price snapshots are recorded (60 seconds).
pub const TWAP_SNAPSHOT_INTERVAL_SECS: u64 = 60;

/// Ring-buffer capacity for TWAP snapshots: window/interval + slack.
pub const TWAP_MAX_SNAPSHOTS: u32 = 34;

// ------------------------------------------------------------
// LIQUIDATION ENGINE
// ------------------------------------------------------------

/// Liquidator bonus at health factor = 1.0 (5%).
pub const BASE_LIQUIDATION_BONUS_BPS: u32 = 500;

/// Maximum dynamic bonus, reached at health factor <= 0.5 (15%).
pub const MAX_DYNAMIC_BONUS_BPS: u32 = 1_500;

/// Hard cap on the liquidation penalty: max 20% of collateral value.
pub const LIQUIDATION_PENALTY_CAP_BPS: u32 = 2_000;

/// Numerator of the target health factor after partial liquidation (1.1).
pub const TARGET_HF_NUM: i128 = 11;

/// Denominator of the target health factor after partial liquidation (1.1).
pub const TARGET_HF_DEN: i128 = 10;

/// Sanity ceiling for admin-configurable liquidation thresholds (95%).
pub const MAX_LIQ_THRESHOLD_BPS: u32 = 9_500;

/// Sanity ceiling for admin-configurable dynamic bonus (25%). The penalty
/// cap still applies on top of this.
pub const MAX_CONFIGURABLE_BONUS_BPS: u32 = 2_500;
