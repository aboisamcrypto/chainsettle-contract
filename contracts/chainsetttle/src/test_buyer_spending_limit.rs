#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Ledger as _, String};

fn sid(env: &Env, id: &str) -> String {
    String::from_str(env, id)
}

#[test]
fn test_get_and_set_buyer_spending_limit() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    assert!(client.get_buyer_spending_limit(&t.buyer2).is_none());

    client.set_buyer_spending_limit(&t.buyer, &t.buyer2, &5_000_000i128, &1000u32);
    assert_eq!(
        client.get_buyer_spending_limit(&t.buyer2),
        Some((5_000_000i128, 1000u32))
    );
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_only_admin_can_set_spending_limit() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_buyer_spending_limit(&t.buyer2, &t.buyer2, &5_000_000i128, &1000u32);
}

#[test]
fn test_buyer_without_limit_unaffected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = sid(&t.env, "spend-unaffected");
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer2),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &9_000_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    assert_eq!(client.get_buyer_spending_window_usage(&t.buyer2), 0);
}

// ============================================================
// CREATE_SHIPMENT ENFORCEMENT
// ============================================================

#[test]
fn test_create_shipment_within_limit_succeeds_and_records_usage() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_buyer_spending_limit(&t.buyer, &t.buyer2, &5_000_000i128, &1000u32);

    let shipment_id = sid(&t.env, "spend-within");
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer2),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &3_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    assert_eq!(client.get_buyer_spending_window_usage(&t.buyer2), 3_000_000);
}

#[test]
#[should_panic(expected = "buyer spending limit exceeded")]
fn test_create_shipment_beyond_limit_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_buyer_spending_limit(&t.buyer, &t.buyer2, &5_000_000i128, &1000u32);

    let shipment_id = sid(&t.env, "spend-over");
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer2),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &5_000_001i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

#[test]
#[should_panic(expected = "buyer spending limit exceeded")]
fn test_cumulative_create_shipments_beyond_limit_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_buyer_spending_limit(&t.buyer, &t.buyer2, &5_000_000i128, &1000u32);

    client.create_shipment(
        &sid(&t.env, "spend-cum-1"),
        &single_buyer_vec(&t.env, &t.buyer2),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &3_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    // 3_000_000 + 3_000_000 = 6_000_000 > 5_000_000 limit within the same window.
    client.create_shipment(
        &sid(&t.env, "spend-cum-2"),
        &single_buyer_vec(&t.env, &t.buyer2),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &3_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

// ============================================================
// TOP_UP_ESCROW ENFORCEMENT
// ============================================================

#[test]
fn test_top_up_within_limit_succeeds() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_buyer_spending_limit(&t.buyer, &t.buyer2, &5_000_000i128, &1000u32);

    let shipment_id = sid(&t.env, "spend-topup-ok");
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer2),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &3_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    client.top_up_escrow(&t.buyer2, &shipment_id, &2_000_000i128);
    assert_eq!(client.get_buyer_spending_window_usage(&t.buyer2), 5_000_000);
}

#[test]
#[should_panic(expected = "buyer spending limit exceeded")]
fn test_top_up_beyond_limit_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_buyer_spending_limit(&t.buyer, &t.buyer2, &5_000_000i128, &1000u32);

    let shipment_id = sid(&t.env, "spend-topup-over");
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer2),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &3_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    client.top_up_escrow(&t.buyer2, &shipment_id, &2_000_001i128);
}

// ============================================================
// ROLLING WINDOW RESET
// ============================================================

#[test]
fn test_usage_resets_once_window_elapses() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_buyer_spending_limit(&t.buyer, &t.buyer2, &5_000_000i128, &100u32);

    client.create_shipment(
        &sid(&t.env, "spend-window-1"),
        &single_buyer_vec(&t.env, &t.buyer2),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &5_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    assert_eq!(client.get_buyer_spending_window_usage(&t.buyer2), 5_000_000);

    // Advance past the window — usage should reset, allowing another full commitment.
    t.env.ledger().with_mut(|l| l.sequence_number += 101);
    assert_eq!(client.get_buyer_spending_window_usage(&t.buyer2), 0);

    client.create_shipment(
        &sid(&t.env, "spend-window-2"),
        &single_buyer_vec(&t.env, &t.buyer2),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &5_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    assert_eq!(client.get_buyer_spending_window_usage(&t.buyer2), 5_000_000);
}
