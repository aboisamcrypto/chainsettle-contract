#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{create_standard_shipment, setup};
use soroban_sdk::{vec, String, Symbol};

fn sid(env: &soroban_sdk::Env, id: &str) -> String {
    String::from_str(env, id)
}

fn proof_hash(env: &soroban_sdk::Env) -> soroban_sdk::String {
    soroban_sdk::String::from_str(env, "QmXyz123")
}

fn proof_type(env: &soroban_sdk::Env) -> Symbol {
    Symbol::new(env, "ipfs")
}

// ============================================================
// #252: coverage for rebalance_milestones
// ============================================================

#[test]
fn test_rebalance_before_any_proof_submission_succeeds() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client, &t.env, &ship_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000,
    );

    let new_percents = vec![&t.env, 10u32, 10u32, 80u32];
    client.rebalance_milestones(&t.buyer, &ship_id, &new_percents);

    let shipment = client.get_shipment(&ship_id);
    assert_eq!(shipment.milestones.get(0).unwrap().payment_percent, 10);
    assert_eq!(shipment.milestones.get(1).unwrap().payment_percent, 10);
    assert_eq!(shipment.milestones.get(2).unwrap().payment_percent, 80);
}

#[test]
#[should_panic(expected = "cannot rebalance: at least one milestone is no longer pending")]
fn test_rebalance_rejected_after_milestone_progressed() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client, &t.env, &ship_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000,
    );

    // Progress milestone 0 past Pending by submitting proof.
    client.submit_proof(&t.supplier, &ship_id, &0u32, &proof_hash(&t.env), &proof_type(&t.env));

    let new_percents = vec![&t.env, 10u32, 10u32, 80u32];
    client.rebalance_milestones(&t.buyer, &ship_id, &new_percents);
}

#[test]
fn test_rebalance_percentages_still_sum_to_100() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client, &t.env, &ship_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000,
    );

    let new_percents = vec![&t.env, 33u32, 33u32, 34u32];
    client.rebalance_milestones(&t.buyer, &ship_id, &new_percents);

    let shipment = client.get_shipment(&ship_id);
    let mut sum: u32 = 0;
    for i in 0..shipment.milestones.len() {
        sum += shipment.milestones.get(i).unwrap().payment_percent;
    }
    assert_eq!(sum, 100);
}

#[test]
#[should_panic(expected = "milestone percentages must sum to 100")]
fn test_rebalance_rejected_when_sum_not_100() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client, &t.env, &ship_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000,
    );

    let new_percents = vec![&t.env, 10u32, 10u32, 70u32];
    client.rebalance_milestones(&t.buyer, &ship_id, &new_percents);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_rebalance_rejected_for_non_buyer_caller() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client, &t.env, &ship_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000,
    );

    let new_percents = vec![&t.env, 10u32, 10u32, 80u32];
    client.rebalance_milestones(&t.supplier, &ship_id, &new_percents);
}
