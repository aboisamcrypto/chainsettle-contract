#![cfg(test)]

extern crate std;

// Shared test fixtures: mock oracles, token setup, and helper functions.

use crate::{LoanManagerContract, LoanManagerContractClient};
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    token, Address, Env,
};

/// 1.0 USD in oracle price decimals.
pub const PRICE_ONE: i128 = 100_000_000;
/// Default liquidation threshold used by fixtures (80%).
pub const DEFAULT_THRESHOLD_BPS: u32 = 8_000;

// ============================================================
// MOCK ORACLE
// ============================================================

#[contracttype]
pub enum MockKey {
    Price(Address),
    Ts(Address),
}

/// Simple price oracle: stores a price + observation timestamp per asset.
/// When no price has been set the call traps, simulating an outage —
/// exactly what the failover chain must tolerate.
#[contract]
pub struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn set_price(env: Env, asset: Address, price: i128) {
        env.storage()
            .instance()
            .set(&MockKey::Price(asset.clone()), &price);
        env.storage()
            .instance()
            .set(&MockKey::Ts(asset), &env.ledger().timestamp());
    }

    /// Force an old timestamp on the stored answer to simulate staleness.
    pub fn set_stale(env: Env, asset: Address, age_secs: u64) {
        let ts = env.ledger().timestamp().saturating_sub(age_secs);
        env.storage().instance().set(&MockKey::Ts(asset), &ts);
    }

    /// Publish an answer with an arbitrary observation timestamp
    /// (e.g. far-future skew or stale data).
    pub fn set_raw(env: Env, asset: Address, price: i128, ts: u64) {
        env.storage()
            .instance()
            .set(&MockKey::Price(asset.clone()), &price);
        env.storage().instance().set(&MockKey::Ts(asset), &ts);
    }

    pub fn latest_price(env: Env, asset: Address) -> (i128, u64) {
        let price: i128 = env
            .storage()
            .instance()
            .get(&MockKey::Price(asset.clone()))
            .unwrap_or_else(|| panic!("price not set"));
        let ts: u64 = env
            .storage()
            .instance()
            .get(&MockKey::Ts(asset))
            .unwrap_or(0);
        (price, ts)
    }
}

// ============================================================
// SETUP
// ============================================================

pub struct Setup {
    pub env: Env,
    pub id: Address,
    pub coll_id: Address,
    pub debt_id: Address,
    pub primary: Address,
    pub secondary: Address,
    pub admin: Address,
    /// Borrower under test.
    pub alice: Address,
    /// Liquidator.
    pub bob: Address,
    /// Liquidity depositors.
    pub d1: Address,
    pub d2: Address,
    pub d3: Address,
}

impl Setup {
    /// Fresh client bound to this setup's environment.
    pub fn client(&self) -> LoanManagerContractClient<'_> {
        LoanManagerContractClient::new(&self.env, &self.id)
    }

    pub fn primary_oracle(&self) -> MockOracleClient<'_> {
        MockOracleClient::new(&self.env, &self.primary)
    }

    pub fn secondary_oracle(&self) -> MockOracleClient<'_> {
        MockOracleClient::new(&self.env, &self.secondary)
    }
}

pub fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    // Start from a realistic unix-ish timestamp so staleness and TWAP
    // window arithmetic behaves like production.
    env.ledger().with_mut(|l| l.timestamp = 1_700_000_000);

    let id = env.register(LoanManagerContract, ());
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let d1 = Address::generate(&env);
    let d2 = Address::generate(&env);
    let d3 = Address::generate(&env);

    // Collateral and debt SAC tokens.
    let coll_admin = Address::generate(&env);
    let coll_id = env.register_stellar_asset_contract_v2(coll_admin).address();
    let debt_admin = Address::generate(&env);
    let debt_id = env.register_stellar_asset_contract_v2(debt_admin).address();

    // Mock oracle chain.
    let primary = env.register(MockOracle, ());
    let secondary = env.register(MockOracle, ());
    let po = MockOracleClient::new(&env, &primary);
    let so = MockOracleClient::new(&env, &secondary);
    po.set_price(&coll_id, &PRICE_ONE);
    po.set_price(&debt_id, &PRICE_ONE);
    so.set_price(&coll_id, &PRICE_ONE);
    so.set_price(&debt_id, &PRICE_ONE);

    let client = LoanManagerContractClient::new(&env, &id);
    client.init(&admin);
    client.set_oracles(&admin, &Some(primary.clone()), &Some(secondary.clone()));
    client.set_market(&admin, &coll_id, &debt_id, &DEFAULT_THRESHOLD_BPS);
    // Seed TWAP history at t0.
    client.record_price(&coll_id);
    client.record_price(&debt_id);

    // Fund participants generously.
    let coll_admin_client = token::StellarAssetClient::new(&env, &coll_id);
    let debt_admin_client = token::StellarAssetClient::new(&env, &debt_id);
    for user in [&alice, &bob] {
        coll_admin_client.mint(user, &100_000_000_000);
        debt_admin_client.mint(user, &100_000_000_000);
    }

    Setup {
        env,
        id,
        coll_id,
        debt_id,
        primary,
        secondary,
        admin,
        alice,
        bob,
        d1,
        d2,
        d3,
    }
}

// ============================================================
// TIME CONTROL
// ============================================================

pub fn advance_time(env: &Env, secs: u64) {
    env.ledger().with_mut(|l| l.timestamp += secs);
}

// ============================================================
// SCENARIO HELPERS
// ============================================================

/// Deposit pool liquidity from each (depositor, amount) pair.
pub fn seed_liquidity(s: &Setup, depositors: &[&Address], amounts: &[i128]) {
    for (d, a) in depositors.iter().zip(amounts.iter()) {
        token::StellarAssetClient::new(&s.env, &s.debt_id).mint(d, a);
        s.client().deposit(d, a);
    }
}

/// Open a healthy position for `user`: collateral `coll` units, borrowing
/// `debt` units (requires HF >= 1 at current prices).
pub fn open_position(s: &Setup, user: &Address, coll: i128, debt: i128) {
    if coll > 0 {
        s.client().deposit_collateral(user, &coll);
    }
    if debt > 0 {
        s.client().borrow(user, &debt);
    }
}
