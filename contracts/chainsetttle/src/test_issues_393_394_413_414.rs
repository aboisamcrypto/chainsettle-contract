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

// ============================================================
// #414 — Blacklist appeal
// ============================================================

#[test]
fn test_blacklist_appeal_approved_removes_blacklist() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let target = Address::generate(&t.env);

    client.blacklist_address(&t.buyer, &target, &soroban_sdk::BytesN::from_array(&t.env, &[1u8; 32]));
    assert!(client.is_blacklisted(&target));

    client.appeal_blacklist(&target, &String::from_str(&t.env, "ipfs://evidence"));
    let appeal = client.get_blacklist_appeal(&target).unwrap();
    assert_eq!(appeal.status, BlacklistAppealStatus::Pending);

    client.review_blacklist_appeal(&t.buyer, &target, &true);
    assert!(!client.is_blacklisted(&target));
    let appeal = client.get_blacklist_appeal(&target).unwrap();
    assert_eq!(appeal.status, BlacklistAppealStatus::Approved);
}

#[test]
fn test_blacklist_appeal_rejected_keeps_blacklist() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let target = Address::generate(&t.env);

    client.blacklist_address(&t.buyer, &target, &soroban_sdk::BytesN::from_array(&t.env, &[1u8; 32]));
    client.appeal_blacklist(&target, &String::from_str(&t.env, "ipfs://evidence"));
    client.review_blacklist_appeal(&t.buyer, &target, &false);

    assert!(client.is_blacklisted(&target));
    let appeal = client.get_blacklist_appeal(&target).unwrap();
    assert_eq!(appeal.status, BlacklistAppealStatus::Rejected);
}

#[test]
#[should_panic(expected = "address is not blacklisted")]
fn test_blacklist_appeal_requires_blacklisted_address() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let target = Address::generate(&t.env);
    client.appeal_blacklist(&target, &String::from_str(&t.env, "ipfs://evidence"));
}

#[test]
#[should_panic(expected = "an appeal is already pending")]
fn test_blacklist_appeal_only_one_pending() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let target = Address::generate(&t.env);
    client.blacklist_address(&t.buyer, &target, &soroban_sdk::BytesN::from_array(&t.env, &[1u8; 32]));
    client.appeal_blacklist(&target, &String::from_str(&t.env, "ipfs://evidence"));
    client.appeal_blacklist(&target, &String::from_str(&t.env, "ipfs://evidence2"));
}

// ============================================================
// #413 — Fee holiday
// ============================================================

#[test]
fn test_fee_holiday_waives_fee_during_window() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_fee_config(&t.buyer, &500, &t.treasury);

    let now = t.env.ledger().sequence();
    client.schedule_fee_holiday(&t.buyer, &now, &(now + 1000));
    assert!(client.is_fee_holiday_active());

    let shipment_id = String::from_str(&t.env, "SHIP-413-A");
    create_standard_shipment(
        &client, &t.env, &shipment_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000_000,
    );

    let token_client = soroban_sdk::token::Client::new(&t.env, &t.token_id);
    let treasury_before = token_client.balance(&t.treasury);

    client.submit_proof(
        &t.supplier, &shipment_id, &0,
        &String::from_str(&t.env, "ipfs://d"), &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    let treasury_after = token_client.balance(&t.treasury);
    assert_eq!(treasury_before, treasury_after);
}

#[test]
fn test_fee_holiday_charges_fee_outside_window() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_fee_config(&t.buyer, &500, &t.treasury);

    let now = t.env.ledger().sequence();
    // Schedule a holiday entirely in the future.
    client.schedule_fee_holiday(&t.buyer, &(now + 500), &(now + 1000));
    assert!(!client.is_fee_holiday_active());

    let shipment_id = String::from_str(&t.env, "SHIP-413-B");
    create_standard_shipment(
        &client, &t.env, &shipment_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000_000,
    );

    let token_client = soroban_sdk::token::Client::new(&t.env, &t.token_id);
    let treasury_before = token_client.balance(&t.treasury);

    client.submit_proof(
        &t.supplier, &shipment_id, &0,
        &String::from_str(&t.env, "ipfs://d"), &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    let treasury_after = token_client.balance(&t.treasury);
    assert!(treasury_after > treasury_before);
}

#[test]
fn test_cancel_fee_holiday() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let now = t.env.ledger().sequence();
    client.schedule_fee_holiday(&t.buyer, &now, &(now + 1000));
    assert!(client.is_fee_holiday_active());
    client.cancel_fee_holiday(&t.buyer);
    assert!(!client.is_fee_holiday_active());
}

// ============================================================
// #394 — Shipment-level custom metadata key-value store
// ============================================================

#[test]
fn test_shipment_metadata_set_and_get() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = String::from_str(&t.env, "SHIP-394-A");
    create_standard_shipment(
        &client, &t.env, &shipment_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000_000,
    );

    let key = Symbol::new(&t.env, "po_number");
    let value = String::from_str(&t.env, "PO-12345");
    client.set_shipment_metadata(&t.buyer, &shipment_id, &key, &value);

    assert_eq!(client.get_shipment_metadata(&shipment_id, &key), Some(value));
    let keys = client.get_shipment_metadata_keys(&shipment_id);
    assert_eq!(keys.len(), 1);
    assert_eq!(keys.get(0).unwrap(), key);
}

#[test]
fn test_shipment_metadata_supplier_can_update() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = String::from_str(&t.env, "SHIP-394-B");
    create_standard_shipment(
        &client, &t.env, &shipment_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000_000,
    );

    let key = Symbol::new(&t.env, "cost_center");
    client.set_shipment_metadata(&t.buyer, &shipment_id, &key, &String::from_str(&t.env, "CC-1"));
    client.set_shipment_metadata(&t.supplier, &shipment_id, &key, &String::from_str(&t.env, "CC-2"));

    assert_eq!(
        client.get_shipment_metadata(&shipment_id, &key),
        Some(String::from_str(&t.env, "CC-2"))
    );
    // Overwriting an existing key must not duplicate the keys index.
    assert_eq!(client.get_shipment_metadata_keys(&shipment_id).len(), 1);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_shipment_metadata_rejects_unrelated_caller() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = String::from_str(&t.env, "SHIP-394-C");
    create_standard_shipment(
        &client, &t.env, &shipment_id, &t.buyer, &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, 1_000_000_000,
    );

    let stranger = Address::generate(&t.env);
    client.set_shipment_metadata(
        &stranger, &shipment_id, &Symbol::new(&t.env, "k"), &String::from_str(&t.env, "v"),
    );
}
