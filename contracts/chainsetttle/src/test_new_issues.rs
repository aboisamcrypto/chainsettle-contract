#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Ledger as _, vec, Env, String, Symbol};

fn sid(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

fn ipfs(env: &Env) -> Symbol {
    Symbol::new(env, "ipfs")
}

// ============================================================
// Issue #42 — Minimum shipment value
// ============================================================

#[test]
fn test_min_shipment_value_default_zero_allows_any_amount() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    // Default min = 0 (disabled) — small shipment must succeed
    client.create_shipment(
        &sid(&t.env, "min-default"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1,
        &vec![
            &t.env,
            Milestone {
                name: sid(&t.env, "M"),
                payment_percent: 100,
                proof_hash: sid(&t.env, ""),
                status: MilestoneStatus::Pending,
                release_after_ledger: 0,
                proof_submitted_ledger: None,
                dispute_opened_ledger: None,
                deadline_ledger: 0,
                penalty_bps_per_ledger: 0,
            },
        ],
        &default_options(&t.env),
    );
    assert_eq!(client.get_min_shipment_value(), 0);
}

#[test]
#[should_panic(expected = "MinShipmentValueNotMet")]
fn test_min_shipment_value_below_floor_panics() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_min_shipment_value(&t.buyer, &1_000);
    // total_amount = 500 < 1_000 → must panic
    client.create_shipment(
        &sid(&t.env, "below-floor"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &500,
        &vec![
            &t.env,
            Milestone {
                name: sid(&t.env, "M"),
                payment_percent: 100,
                proof_hash: sid(&t.env, ""),
                status: MilestoneStatus::Pending,
                release_after_ledger: 0,
                proof_submitted_ledger: None,
                dispute_opened_ledger: None,
                deadline_ledger: 0,
                penalty_bps_per_ledger: 0,
            },
        ],
        &default_options(&t.env),
    );
}

#[test]
fn test_min_shipment_value_at_floor_succeeds() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_min_shipment_value(&t.buyer, &1_000);
    // total_amount == floor → must succeed
    client.create_shipment(
        &sid(&t.env, "at-floor"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000,
        &vec![
            &t.env,
            Milestone {
                name: sid(&t.env, "M"),
                payment_percent: 100,
                proof_hash: sid(&t.env, ""),
                status: MilestoneStatus::Pending,
                release_after_ledger: 0,
                proof_submitted_ledger: None,
                dispute_opened_ledger: None,
                deadline_ledger: 0,
                penalty_bps_per_ledger: 0,
            },
        ],
        &default_options(&t.env),
    );
    assert_eq!(
        client.get_shipment(&sid(&t.env, "at-floor")).status,
        ShipmentStatus::Active
    );
}

#[test]
fn test_top_up_escrow_not_gated_by_min_shipment_value() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    // Create shipment at floor
    client.set_min_shipment_value(&t.buyer, &1_000);
    client.create_shipment(
        &sid(&t.env, "topup-floor"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    // top_up with a tiny amount must succeed (floor only gates creation)
    client.top_up_escrow(&t.buyer, &sid(&t.env, "topup-floor"), &1);
    assert_eq!(
        client
            .get_shipment(&sid(&t.env, "topup-floor"))
            .total_amount,
        1_001
    );
}

#[test]
fn test_min_and_max_shipment_value_coexist() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_min_shipment_value(&t.buyer, &500);
    client.set_max_shipment_value(&t.buyer, &10_000);
    // amount in [500, 10_000] → success
    client.create_shipment(
        &sid(&t.env, "in-range"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &5_000,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    assert_eq!(
        client.get_shipment(&sid(&t.env, "in-range")).status,
        ShipmentStatus::Active
    );
}

// ============================================================
// Issue #299 — Shipment-level fee override
// ============================================================

#[test]
fn test_shipment_fee_override_applied() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    // Global fee = 200 bps (2%)
    client.set_fee_config(&t.buyer, &200u32, &t.treasury);
    let ship_id = sid(&t.env, "fee-override");
    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    // Override to 0 bps for this shipment
    client.set_shipment_fee_override(&t.buyer, &ship_id, &0u32);
    assert_eq!(client.get_shipment_fee_override(&ship_id), Some(0u32));
}

#[test]
fn test_shipment_fee_override_cleared_reverts_to_global() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "fee-clear");
    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    client.set_shipment_fee_override(&t.buyer, &ship_id, &50u32);
    client.clear_shipment_fee_override(&t.buyer, &ship_id);
    assert_eq!(client.get_shipment_fee_override(&ship_id), None);
}

#[test]
fn test_no_fee_override_returns_none() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "no-override");
    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    assert_eq!(client.get_shipment_fee_override(&ship_id), None);
}

#[test]
fn test_other_shipments_unaffected_by_override() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_a = sid(&t.env, "ship-a");
    let ship_b = sid(&t.env, "ship-b");
    for id in [&ship_a, &ship_b] {
        client.create_shipment(
            id,
            &single_buyer_vec(&t.env, &t.buyer),
            &t.supplier,
            &t.logistics,
            &t.arbiter,
            &t.token_id,
            &1_000_000,
            &build_milestones(&t.env),
            &default_options(&t.env),
        );
    }
    client.set_shipment_fee_override(&t.buyer, &ship_a, &10u32);
    // ship_b must be unaffected
    assert_eq!(client.get_shipment_fee_override(&ship_b), None);
}

// ============================================================
// Issue #300 — Long-hold escrow rebate
// ============================================================

#[test]
fn test_long_hold_rebate_disabled_by_default() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    assert_eq!(client.get_long_hold_rebate(), (0u32, 0u32));
}

#[test]
fn test_long_hold_rebate_getter_after_set() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_long_hold_rebate(&t.buyer, &1000u32, &500u32);
    assert_eq!(client.get_long_hold_rebate(), (1000u32, 500u32));
}

#[test]
fn test_long_hold_rebate_zero_bps_no_change_to_fee() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    // rebate_bps = 0 → disabled, fee behaviour unchanged
    client.set_fee_config(&t.buyer, &100u32, &t.treasury);
    client.set_long_hold_rebate(&t.buyer, &0u32, &0u32);
    let ship_id = sid(&t.env, "rebate-disabled");
    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h"),
        &ipfs(&t.env),
    );
    // confirm should succeed without rebate
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);
    assert_eq!(client.get_shipment(&ship_id).status, ShipmentStatus::Active);
}

// ============================================================
// Issue #298 — Governance timelock
// ============================================================

#[test]
fn test_propose_and_execute_param_change_after_delay() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    // Set timelock of 10 ledgers
    client.set_timelock_duration(&t.buyer, &10u32);
    let param = Symbol::new(&t.env, "max_shipment_value");
    client.propose_param_change(&t.buyer, &param, &999_999);
    // Advance ledger past timelock
    t.env.ledger().with_mut(|l| l.sequence_number += 11);
    client.execute_param_change(&t.buyer, &param);
    assert_eq!(client.get_max_shipment_value(), 999_999);
}

#[test]
#[should_panic(expected = "TimelockNotExpired")]
fn test_execute_param_change_before_delay_panics() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_timelock_duration(&t.buyer, &100u32);
    let param = Symbol::new(&t.env, "max_shipment_value");
    client.propose_param_change(&t.buyer, &param, &500_000);
    // Do NOT advance ledger — must panic
    client.execute_param_change(&t.buyer, &param);
}

#[test]
#[should_panic(expected = "NoPendingParamChange")]
fn test_cancel_param_change_removes_pending() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_timelock_duration(&t.buyer, &50u32);
    let param = Symbol::new(&t.env, "max_shipment_value");
    client.propose_param_change(&t.buyer, &param, &1_000_000);
    client.cancel_param_change(&t.buyer, &param);
    // Advance past timelock and try to execute — must panic (no pending change)
    t.env.ledger().with_mut(|l| l.sequence_number += 51);
    client.execute_param_change(&t.buyer, &param);
}

#[test]
fn test_timelock_zero_executes_immediately() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    // Default timelock = 0 → effective_ledger = current → can execute immediately
    let param = Symbol::new(&t.env, "max_shipment_value");
    client.propose_param_change(&t.buyer, &param, &777_777);
    client.execute_param_change(&t.buyer, &param);
    assert_eq!(client.get_max_shipment_value(), 777_777);
}

#[test]
fn test_circuit_breaker_limit_param_change() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let param = Symbol::new(&t.env, "circuit_breaker_limit");
    client.propose_param_change(&t.buyer, &param, &5_000_000);
    client.execute_param_change(&t.buyer, &param);
    // No panic = success; circuit breaker limit updated
}
