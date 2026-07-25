#![cfg(test)]
// #167 — Structured on-chain event log.
//
// Verifies the seven canonical events documented in docs/events.md are emitted,
// using the two-topic form (Symbol("chainsettle"), Symbol(event_name)) with a
// Map<Symbol, Val> data payload, at the correct lifecycle transitions.

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Events as _, Map, String, Symbol, TryFromVal, Val};

fn sid(env: &Env, id: &str) -> String {
    String::from_str(env, id)
}

/// Find the most recent chainsettle-schema event with the given name and decode its data map.
fn find_chainsettle_event(env: &Env, name: &str) -> Option<Map<Symbol, Val>> {
    let events = env.events().all();
    let expected_ns = Symbol::new(env, "chainsettle");
    let expected_name = Symbol::new(env, name);
    let mut found: Option<Map<Symbol, Val>> = None;
    for e in events.iter() {
        let topics = e.1.clone();
        if topics.len() == 2 {
            let t0 = Symbol::try_from_val(env, &topics.get(0).unwrap());
            let t1 = Symbol::try_from_val(env, &topics.get(1).unwrap());
            if let (Ok(t0), Ok(t1)) = (t0, t1) {
                if t0 == expected_ns && t1 == expected_name {
                    found = Some(Map::<Symbol, Val>::try_from_val(env, &e.2).unwrap());
                }
            }
        }
    }
    found
}

fn field_string(env: &Env, data: &Map<Symbol, Val>, key: &str) -> String {
    String::try_from_val(env, &data.get(Symbol::new(env, key)).unwrap()).unwrap()
}

fn field_address(env: &Env, data: &Map<Symbol, Val>, key: &str) -> Address {
    Address::try_from_val(env, &data.get(Symbol::new(env, key)).unwrap()).unwrap()
}

fn field_i128(env: &Env, data: &Map<Symbol, Val>, key: &str) -> i128 {
    i128::try_from_val(env, &data.get(Symbol::new(env, key)).unwrap()).unwrap()
}

fn field_u32(env: &Env, data: &Map<Symbol, Val>, key: &str) -> u32 {
    u32::try_from_val(env, &data.get(Symbol::new(env, key)).unwrap()).unwrap()
}

fn field_symbol(env: &Env, data: &Map<Symbol, Val>, key: &str) -> Symbol {
    Symbol::try_from_val(env, &data.get(Symbol::new(env, key)).unwrap()).unwrap()
}

#[test]
fn test_shipment_created_event_schema() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = sid(&t.env, "evt-created");

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    let data = find_chainsettle_event(&t.env, "shipment_created")
        .expect("ShipmentCreated must be emitted by create_shipment");
    assert_eq!(field_string(&t.env, &data, "shipment_id"), shipment_id);
    assert_eq!(field_address(&t.env, &data, "buyer"), t.buyer);
    assert_eq!(field_address(&t.env, &data, "supplier"), t.supplier);
    assert_eq!(field_address(&t.env, &data, "arbiter"), t.arbiter);
    assert_eq!(field_address(&t.env, &data, "token"), t.token_id);
    assert_eq!(field_i128(&t.env, &data, "amount"), 1_000_000_000i128);
}

#[test]
fn test_milestone_proof_submitted_event_schema() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = sid(&t.env, "evt-proof");

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "ipfs://proof0"),
        &Symbol::new(&t.env, "ipfs"),
    );

    let data = find_chainsettle_event(&t.env, "proof_submitted")
        .expect("MilestoneProofSubmitted must be emitted by submit_proof");
    assert_eq!(field_string(&t.env, &data, "shipment_id"), shipment_id);
    assert_eq!(field_u32(&t.env, &data, "milestone_index"), 0);
    assert_eq!(
        field_string(&t.env, &data, "proof_hash"),
        String::from_str(&t.env, "ipfs://proof0")
    );
    assert_eq!(field_address(&t.env, &data, "supplier"), t.supplier);
}

#[test]
fn test_milestone_confirmed_and_shipment_completed_event_schema() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = sid(&t.env, "evt-confirm");
    let total_amount = 1_000_000_000i128;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env), // 25 / 50 / 25
        &default_options(&t.env),
    );

    // Confirm milestone 0 (25%) — shipment not yet complete.
    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "proof0"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    let data = find_chainsettle_event(&t.env, "milestone_confirmed")
        .expect("MilestoneConfirmed must be emitted by confirm_milestone");
    assert_eq!(field_string(&t.env, &data, "shipment_id"), shipment_id);
    assert_eq!(field_u32(&t.env, &data, "milestone_index"), 0);
    assert_eq!(field_i128(&t.env, &data, "payout_amount"), 250_000_000);
    assert!(
        find_chainsettle_event(&t.env, "shipment_completed").is_none(),
        "ShipmentCompleted must not fire until every milestone is confirmed"
    );

    // Confirm the remaining two milestones to complete the shipment.
    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &1,
        &String::from_str(&t.env, "proof1"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &1);
    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &2,
        &String::from_str(&t.env, "proof2"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &2);

    let completed = find_chainsettle_event(&t.env, "shipment_completed")
        .expect("ShipmentCompleted must be emitted once the final milestone is confirmed");
    assert_eq!(field_string(&t.env, &completed, "shipment_id"), shipment_id);
    assert_eq!(field_i128(&t.env, &completed, "total_paid"), total_amount);
}

#[test]
fn test_dispute_opened_and_resolved_event_schema() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = sid(&t.env, "evt-dispute");

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "proof0"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.raise_dispute(&t.buyer, &shipment_id, &0);

    let opened = find_chainsettle_event(&t.env, "dispute_opened")
        .expect("DisputeOpened must be emitted by raise_dispute");
    assert_eq!(field_string(&t.env, &opened, "shipment_id"), shipment_id);
    assert_eq!(field_u32(&t.env, &opened, "milestone_index"), 0);
    assert_eq!(field_address(&t.env, &opened, "buyer"), t.buyer);

    client.resolve_dispute(&t.arbiter, &shipment_id, &0, &true);

    let resolved = find_chainsettle_event(&t.env, "dispute_resolved")
        .expect("DisputeResolved must be emitted by resolve_dispute");
    assert_eq!(field_string(&t.env, &resolved, "shipment_id"), shipment_id);
    assert_eq!(field_u32(&t.env, &resolved, "milestone_index"), 0);
    assert_eq!(
        field_symbol(&t.env, &resolved, "resolution"),
        Symbol::new(&t.env, "supplier")
    );
    assert_eq!(field_address(&t.env, &resolved, "resolver"), t.arbiter);
}

#[test]
fn test_shipment_cancelled_event_schema() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = sid(&t.env, "evt-cancel");
    let total_amount = 1_000_000_000i128;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    client.cancel_shipment(&t.buyer, &shipment_id);

    let data = find_chainsettle_event(&t.env, "shipment_cancelled")
        .expect("ShipmentCancelled must be emitted by cancel_shipment");
    assert_eq!(field_string(&t.env, &data, "shipment_id"), shipment_id);
    assert_eq!(field_i128(&t.env, &data, "refund_amount"), total_amount);
}
