#![cfg(test)]

//! Structured cancellation reason codes on shipment_cancelled + Shipment record.

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{
    testutils::{Events as _, Ledger as _},
    vec, Map, String, Symbol, TryFromVal, Val,
};

fn sid(env: &Env, id: &str) -> String {
    String::from_str(env, id)
}

fn find_cancelled_event(env: &Env) -> Option<Map<Symbol, Val>> {
    let events = env.events().all();
    let expected_ns = Symbol::new(env, "chainsettle");
    let expected_name = Symbol::new(env, "shipment_cancelled");
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

fn event_reason(env: &Env, data: &Map<Symbol, Val>) -> Symbol {
    Symbol::try_from_val(env, &data.get(Symbol::new(env, "reason")).unwrap()).unwrap()
}

fn event_refund(env: &Env, data: &Map<Symbol, Val>) -> i128 {
    i128::try_from_val(env, &data.get(Symbol::new(env, "refund_amount")).unwrap()).unwrap()
}

fn event_shipment_id(env: &Env, data: &Map<Symbol, Val>) -> String {
    String::try_from_val(env, &data.get(Symbol::new(env, "shipment_id")).unwrap()).unwrap()
}

#[test]
fn test_buyer_cancel_reason() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = sid(&t.env, "reason-buyer");
    let total = 1_000_000_000i128;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    client.cancel_shipment(&t.buyer, &shipment_id);

    let data = find_cancelled_event(&t.env).expect("shipment_cancelled must fire");
    assert_eq!(event_shipment_id(&t.env, &data), shipment_id);
    assert_eq!(event_refund(&t.env, &data), total);
    assert_eq!(
        event_reason(&t.env, &data),
        Symbol::new(&t.env, "BuyerCancelled")
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
    assert_eq!(shipment.cancellation_reason.len(), 1);
    assert_eq!(
        shipment.cancellation_reason.get(0).unwrap(),
        CancellationReason::BuyerCancelled
    );
}

#[test]
fn test_supplier_cancel_reason() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = sid(&t.env, "reason-supplier");
    let total = 1_000_000_000i128;
    let deadline = 100u32;

    let mut opts = default_options(&t.env);
    opts.response_deadline = deadline;
    opts.penalty_bps = 0;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total,
        &build_milestones(&t.env),
        &opts,
    );

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://p"),
        &Symbol::new(&t.env, "ipfs"),
    );
    // Keep instance alive across the deadline jump.
    t.env.as_contract(&t.contract_id, || {
        t.env.storage().instance().extend_ttl(100_000, 6_300_000);
    });
    t.env.ledger().set_sequence_number(deadline + 1);

    client.supplier_cancel(&t.supplier, &shipment_id);

    let data = find_cancelled_event(&t.env).expect("shipment_cancelled must fire");
    assert_eq!(
        event_reason(&t.env, &data),
        Symbol::new(&t.env, "SupplierCancelled")
    );
    assert_eq!(event_refund(&t.env, &data), total);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
    assert_eq!(shipment.cancellation_reason.len(), 1);
    assert_eq!(
        shipment.cancellation_reason.get(0).unwrap(),
        CancellationReason::SupplierCancelled
    );
}

#[test]
fn test_deadline_refund_reason() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = sid(&t.env, "reason-deadline");
    let total = 1_000_000_000i128;

    let mut opts = default_options(&t.env);
    opts.deadlines = vec![&t.env, 1_000u64, 1_000u64, 1_000u64];

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total,
        &build_milestones(&t.env),
        &opts,
    );

    t.env.ledger().with_mut(|li| li.timestamp = 2_000);
    client.claim_deadline_refund(&t.buyer, &shipment_id, &0);

    let data = find_cancelled_event(&t.env).expect("shipment_cancelled must fire on deadline refund");
    assert_eq!(
        event_reason(&t.env, &data),
        Symbol::new(&t.env, "DeadlineRefund")
    );
    assert_eq!(event_refund(&t.env, &data), total);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Expired);
    assert_eq!(shipment.cancellation_reason.len(), 1);
    assert_eq!(
        shipment.cancellation_reason.get(0).unwrap(),
        CancellationReason::DeadlineRefund
    );
}

#[test]
fn test_emergency_recover_reason() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = sid(&t.env, "reason-emergency");
    let total = 1_000_000_000i128;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    // Advance past the test-build recovery threshold (100 ledgers).
    let created = client.get_shipment(&shipment_id).created_at;
    t.env.as_contract(&t.contract_id, || {
        t.env.storage().instance().extend_ttl(100_000, 6_300_000);
    });
    t.env
        .ledger()
        .set_sequence_number(created + RECOVERY_THRESHOLD_LEDGERS + 1);

    client.emergency_recover(&t.buyer, &shipment_id); // buyer is admin from init

    let data =
        find_cancelled_event(&t.env).expect("shipment_cancelled must fire on emergency recover");
    assert_eq!(
        event_reason(&t.env, &data),
        Symbol::new(&t.env, "AdminEmergencyRecovery")
    );
    assert_eq!(event_refund(&t.env, &data), total);

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Cancelled);
    assert_eq!(shipment.cancellation_reason.len(), 1);
    assert_eq!(
        shipment.cancellation_reason.get(0).unwrap(),
        CancellationReason::AdminEmergencyRecovery
    );
}
