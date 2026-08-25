#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{create_standard_shipment, setup, TestSetup};
use soroban_sdk::testutils::{Events, Ledger};
use soroban_sdk::{String, Symbol, TryFromVal};

fn sid(env: &soroban_sdk::Env, id: &str) -> String {
    String::from_str(env, id)
}

fn proof_hash(env: &soroban_sdk::Env) -> soroban_sdk::String {
    soroban_sdk::String::from_str(env, "QmXyz123")
}

fn proof_type(env: &soroban_sdk::Env) -> Symbol {
    Symbol::new(env, "ipfs")
}

fn open_dispute(t: &TestSetup, client: &ChainSettleContractClient, ship_id: &String) {
    client.submit_proof(
        &t.supplier,
        ship_id,
        &0u32,
        &proof_hash(&t.env),
        &proof_type(&t.env),
    );
    client.raise_dispute(&t.buyer, ship_id, &0u32);
}

// ============================================================
// #251: coverage for check_escalation / escalation threshold
// ============================================================

#[test]
fn test_set_and_get_escalation_threshold() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    assert_eq!(client.get_escalation_threshold(), 0);
    client.set_escalation_threshold(&t.buyer, &500u32);
    assert_eq!(client.get_escalation_threshold(), 500);
}

#[test]
fn test_check_escalation_under_threshold_no_escalation() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client,
        &t.env,
        &ship_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        1_000_000,
    );

    client.set_escalation_threshold(&t.buyer, &100u32);
    open_dispute(&t, &client, &ship_id);

    // Advance the ledger, but stay under the escalation threshold.
    t.env
        .ledger()
        .set_sequence_number(t.env.ledger().sequence() + 50);

    client.check_escalation(&ship_id, &0u32);
    // events().all() reflects only the most recent top-level call, so this
    // asserts check_escalation itself published nothing.
    assert_eq!(
        t.env.events().all().len(),
        0,
        "no escalation event should fire before threshold elapses"
    );

    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::Disputed);
}

#[test]
fn test_check_escalation_past_threshold_triggers_event_and_preserves_state() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client,
        &t.env,
        &ship_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        1_000_000,
    );

    client.set_escalation_threshold(&t.buyer, &100u32);
    open_dispute(&t, &client, &ship_id);
    let opened_ledger = client
        .get_milestone(&ship_id, &0u32)
        .dispute_opened_ledger
        .unwrap();

    // Advance the ledger past the escalation threshold.
    t.env.ledger().set_sequence_number(opened_ledger + 100);

    client.check_escalation(&ship_id, &0u32);
    let events = t.env.events().all();
    assert_eq!(events.len(), 1, "exactly one escalation event should fire");

    let (_contract_id, topics, _data) = events.get(0).unwrap();
    let topic0: Symbol = Symbol::try_from_val(&t.env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(topic0, Symbol::new(&t.env, "dispute_escalated"));

    // check_escalation only emits an event; dispute state itself is unchanged.
    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::Disputed);
    assert_eq!(milestone.dispute_opened_ledger, Some(opened_ledger));
}

#[test]
fn test_check_escalation_disabled_by_default_no_event() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client,
        &t.env,
        &ship_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        1_000_000,
    );

    // Threshold left at default (0 = disabled).
    open_dispute(&t, &client, &ship_id);
    t.env
        .ledger()
        .set_sequence_number(t.env.ledger().sequence() + 1_000_000);

    client.check_escalation(&ship_id, &0u32);
    assert_eq!(t.env.events().all().len(), 0);
}

#[test]
fn test_check_escalation_non_disputed_milestone_no_event() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "ship1");
    create_standard_shipment(
        &client,
        &t.env,
        &ship_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        1_000_000,
    );

    client.set_escalation_threshold(&t.buyer, &1u32);
    t.env
        .ledger()
        .set_sequence_number(t.env.ledger().sequence() + 1000);

    // Milestone 0 is still Pending, never disputed.
    client.check_escalation(&ship_id, &0u32);
    assert_eq!(t.env.events().all().len(), 0);
}
