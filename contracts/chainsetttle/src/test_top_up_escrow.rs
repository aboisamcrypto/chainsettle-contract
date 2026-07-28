#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{create_standard_shipment, setup, TestSetup};
use soroban_sdk::{String, Symbol};

fn sid(env: &soroban_sdk::Env, id: &str) -> String {
    String::from_str(env, id)
}

fn proof_hash(env: &soroban_sdk::Env) -> soroban_sdk::String {
    soroban_sdk::String::from_str(env, "QmXyz123")
}

fn proof_type(env: &soroban_sdk::Env) -> Symbol {
    Symbol::new(env, "ipfs")
}

/// Drives all three milestones of a standard shipment to Confirmed, completing it.
fn complete_shipment(t: &TestSetup, client: &ChainSettleContractClient, ship_id: &String) {
    for i in 0..3u32 {
        client.submit_proof(&t.supplier, ship_id, &i, &proof_hash(&t.env), &proof_type(&t.env));
        client.confirm_milestone(&t.buyer, ship_id, &i);
    }
}

// ============================================================
// #253: coverage for top_up_escrow
// ============================================================

#[test]
fn test_buyer_tops_up_active_shipment_increases_total_amount() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client, &t.env, &ship_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000,
    );

    client.top_up_escrow(&t.buyer, &ship_id, &500_000);

    let shipment = client.get_shipment(&ship_id);
    assert_eq!(shipment.total_amount, 1_500_000);
    assert_eq!(shipment.status, ShipmentStatus::Active);
}

#[test]
fn test_top_up_total_amount_increases_correctly_across_multiple_calls() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client, &t.env, &ship_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000,
    );

    client.top_up_escrow(&t.buyer, &ship_id, &200_000);
    client.top_up_escrow(&t.buyer, &ship_id, &300_000);

    let shipment = client.get_shipment(&ship_id);
    assert_eq!(shipment.total_amount, 1_500_000);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_top_up_rejected_for_non_buyer_caller() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client, &t.env, &ship_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000,
    );

    client.top_up_escrow(&t.supplier, &ship_id, &500_000);
}

#[test]
#[should_panic(expected = "top-up disallowed: shipment is not active")]
fn test_top_up_rejected_on_cancelled_shipment() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client, &t.env, &ship_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000,
    );

    client.cancel_shipment(&t.buyer, &ship_id);
    client.top_up_escrow(&t.buyer, &ship_id, &500_000);
}

#[test]
#[should_panic(expected = "top-up disallowed: shipment is not active")]
fn test_top_up_rejected_on_completed_shipment() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client, &t.env, &ship_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000,
    );

    complete_shipment(&t, &client, &ship_id);
    assert_eq!(client.get_shipment(&ship_id).status, ShipmentStatus::Completed);

    client.top_up_escrow(&t.buyer, &ship_id, &500_000);
}

#[test]
#[should_panic(expected = "additional_amount must be greater than zero")]
fn test_top_up_rejects_non_positive_amount() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client, &t.env, &ship_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000,
    );

    client.top_up_escrow(&t.buyer, &ship_id, &0);
}
