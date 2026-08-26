#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, String, Symbol,
};

// ============================================================
// #393 — Resolution finality delay
// ============================================================

#[test]
fn test_finality_delay_holds_funds_until_elapsed() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_finality_delay_ledgers(&t.buyer, &50);

    let shipment_id = String::from_str(&t.env, "SHIP-393-A");
    create_standard_shipment(
        &client, &t.env, &shipment_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000_000,
    );

    client.submit_proof(
        &t.supplier, &shipment_id, &0,
        &String::from_str(&t.env, "ipfs://d"), &Symbol::new(&t.env, "ipfs"),
    );
    client.raise_dispute(&t.buyer, &shipment_id, &0);
    client.resolve_dispute(&t.arbiter, &shipment_id, &0, &true);

    let shipment = client.get_shipment(&shipment_id);
    let milestone = shipment.milestones.get(0).unwrap();
    assert_eq!(milestone.status, MilestoneStatus::ResolvedPendingFinality);
    assert_eq!(shipment.released_amount, 0);

    // Too early
    let result = client.try_finalize_dispute_resolution(&shipment_id, &0);
    assert!(result.is_err());

    // Advance past the delay
    let target = milestone.release_after_ledger;
    t.env.ledger().set_sequence_number(target);
    client.finalize_dispute_resolution(&shipment_id, &0);

    let shipment = client.get_shipment(&shipment_id);
    let milestone = shipment.milestones.get(0).unwrap();
    assert_eq!(milestone.status, MilestoneStatus::Resolved);
    assert!(shipment.released_amount > 0);
}

#[test]
fn test_finality_delay_disabled_releases_immediately() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    // No delay configured (default 0).
    let shipment_id = String::from_str(&t.env, "SHIP-393-B");
    create_standard_shipment(
        &client, &t.env, &shipment_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000_000,
    );

    client.submit_proof(
        &t.supplier, &shipment_id, &0,
        &String::from_str(&t.env, "ipfs://d"), &Symbol::new(&t.env, "ipfs"),
    );
    client.raise_dispute(&t.buyer, &shipment_id, &0);
    client.resolve_dispute(&t.arbiter, &shipment_id, &0, &true);

    let shipment = client.get_shipment(&shipment_id);
    let milestone = shipment.milestones.get(0).unwrap();
    assert_eq!(milestone.status, MilestoneStatus::Resolved);
    assert!(shipment.released_amount > 0);
}

#[test]
fn test_finality_delay_buyer_can_still_appeal_before_finalize() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_finality_delay_ledgers(&t.buyer, &50);
    client.set_appeal_window_ledgers(&t.buyer, &100);
    let arbiter2 = Address::generate(&t.env);
    client.add_arbiter_to_pool(&t.buyer, &arbiter2);

    let shipment_id = String::from_str(&t.env, "SHIP-393-C");
    create_standard_shipment(
        &client, &t.env, &shipment_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000_000,
    );

    client.submit_proof(
        &t.supplier, &shipment_id, &0,
        &String::from_str(&t.env, "ipfs://d"), &Symbol::new(&t.env, "ipfs"),
    );
    client.raise_dispute(&t.buyer, &shipment_id, &0);
    client.resolve_dispute(&t.arbiter, &shipment_id, &0, &true);

    // Buyer catches the error and appeals before finalize_dispute_resolution.
    client.appeal_dispute(&t.buyer, &shipment_id, &0);

    let shipment = client.get_shipment(&shipment_id);
    let milestone = shipment.milestones.get(0).unwrap();
    assert_eq!(milestone.status, MilestoneStatus::Disputed);
    assert_eq!(shipment.released_amount, 0);
}
