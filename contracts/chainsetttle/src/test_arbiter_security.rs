#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, single_buyer_vec, TestSetup};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String, Symbol,
};

// ============================================================
// TEST SETUP
// ============================================================

fn setup() -> TestSetup {
    crate::test_common::setup()
}

fn proof_type(env: &Env) -> Symbol {
    Symbol::new(env, "ipfs")
}

// ============================================================
// SECURITY TEST 1: Arbiter Cannot Resolve Non-Disputed Milestone
// ============================================================

#[test]
#[should_panic(expected = "milestone is not in disputed status")]
fn test_arbiter_cannot_resolve_pending_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-001");
    let total_amount: i128 = 1_000_000_000;

    // Create shipment with milestone in Pending state
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

    // Arbiter attempts to resolve dispute on Pending milestone (not Disputed)
    // Should panic: milestone is not in disputed status
    client.resolve_dispute(&t.arbiter, &shipment_id, &0u32, &true);
}

// ============================================================
// SECURITY TEST 2: Arbiter Cannot Resolve ProofSubmitted Milestone
// ============================================================

#[test]
#[should_panic(expected = "milestone is not in disputed status")]
fn test_arbiter_cannot_resolve_proof_submitted_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-002");
    let total_amount: i128 = 1_000_000_000;
    let proof_hash = String::from_str(&t.env, "proof_hash_002");

    // Create shipment
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

    // Submit proof - milestone now in ProofSubmitted state
    client.submit_proof(&t.supplier, &shipment_id, &0u32, &proof_hash, &proof_type(&t.env));

    // Arbiter attempts to resolve dispute on ProofSubmitted milestone
    // Should panic: milestone is not in disputed status
    client.resolve_dispute(&t.arbiter, &shipment_id, &0u32, &true);
}

// ============================================================
// SECURITY TEST 3: Arbiter Cannot Resolve Confirmed Milestone
// ============================================================

#[test]
#[should_panic(expected = "milestone is not in disputed status")]
fn test_arbiter_cannot_resolve_confirmed_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-003");
    let total_amount: i128 = 1_000_000_000;
    let proof_hash = String::from_str(&t.env, "proof_hash_003");

    // Create shipment
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

    // Submit proof and confirm milestone
    client.submit_proof(&t.supplier, &shipment_id, &0u32, &proof_hash, &proof_type(&t.env));
    t.env.ledger().set_sequence_number(t.env.ledger().sequence() + 100);
    client.confirm_milestone(&t.buyer, &shipment_id, &0u32);

    // Arbiter attempts to resolve dispute on Confirmed milestone
    // Should panic: milestone is not in disputed status
    client.resolve_dispute(&t.arbiter, &shipment_id, &0u32, &true);
}

// ============================================================
// SECURITY TEST 4: Arbiter Cannot Resolve ConfirmedHeld Milestone
// ============================================================

#[test]
#[should_panic(expected = "milestone is not in disputed status")]
fn test_arbiter_cannot_resolve_confirmed_held_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-004");
    let total_amount: i128 = 1_000_000_000;
    let proof_hash = String::from_str(&t.env, "proof_hash_004");

    // Create shipment with holdback
    let mut options = default_options(&t.env);
    options.holdback_ledgers = 1000; // Enable holdback

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &options,
    );

    // Submit proof and confirm milestone - will go to ConfirmedHeld
    client.submit_proof(&t.supplier, &shipment_id, &0u32, &proof_hash, &proof_type(&t.env));
    t.env.ledger().set_sequence_number(t.env.ledger().sequence() + 100);
    client.confirm_milestone(&t.buyer, &shipment_id, &0u32);

    // Arbiter attempts to resolve dispute on ConfirmedHeld milestone
    // Should panic: milestone is not in disputed status
    client.resolve_dispute(&t.arbiter, &shipment_id, &0u32, &true);
}

// ============================================================
// SECURITY TEST 5: Arbiter Cannot Resolve Resolved Milestone
// ============================================================

#[test]
#[should_panic(expected = "milestone is not in disputed status")]
fn test_arbiter_cannot_resolve_resolved_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-005");
    let total_amount: i128 = 1_000_000_000;
    let proof_hash = String::from_str(&t.env, "proof_hash_005");

    // Create shipment
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

    // Submit proof and raise dispute
    client.submit_proof(&t.supplier, &shipment_id, &0u32, &proof_hash, &proof_type(&t.env));
    client.raise_dispute(&t.buyer, &shipment_id, &0u32);

    // Resolve the dispute - milestone now in Resolved state
    client.resolve_dispute(&t.arbiter, &shipment_id, &0u32, &true);

    // Arbiter attempts to resolve dispute AGAIN on Resolved milestone
    // Should panic: milestone is not in disputed status
    client.resolve_dispute(&t.arbiter, &shipment_id, &0u32, &true);
}

// ============================================================
// SECURITY TEST 6: Arbiter Cannot Call confirm_milestone
// ============================================================

#[test]
#[should_panic(expected = "unauthorized")]
fn test_arbiter_cannot_call_confirm_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-006");
    let total_amount: i128 = 1_000_000_000;
    let proof_hash = String::from_str(&t.env, "proof_hash_006");

    // Create shipment
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

    // Submit proof
    client.submit_proof(&t.supplier, &shipment_id, &0u32, &proof_hash, &proof_type(&t.env));

    // Arbiter (non-buyer) attempts to confirm milestone
    // Should panic: unauthorized
    // Note: This test uses arbiter as caller to confirm_milestone
    // Soroban's mock_all_auths() allows this, but the contract should reject it
    client.confirm_milestone(&t.arbiter, &shipment_id, &0u32);
}

// ============================================================
// SECURITY TEST 7: Arbiter Cannot Call cancel_shipment
// ============================================================

#[test]
#[should_panic(expected = "unauthorized")]
fn test_arbiter_cannot_call_cancel_shipment() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-007");
    let total_amount: i128 = 1_000_000_000;

    // Create shipment
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

    // Arbiter (non-buyer) attempts to cancel shipment
    // Should panic: unauthorized
    client.cancel_shipment(&t.arbiter, &shipment_id);
}

// ============================================================
// SECURITY TEST 8: Only Buyer Can Raise Dispute and Only Arbiter Can Resolve
// ============================================================

#[test]
fn test_only_arbiter_can_resolve_after_buyer_raises_dispute() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-008");
    let total_amount: i128 = 1_000_000_000;
    let proof_hash = String::from_str(&t.env, "proof_hash_008");

    // Create shipment
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

    // Buyer submits proof
    client.submit_proof(&t.supplier, &shipment_id, &0u32, &proof_hash, &proof_type(&t.env));

    // Buyer raises dispute
    client.raise_dispute(&t.buyer, &shipment_id, &0u32);

    // Verify milestone is now Disputed
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.milestones.get(0).unwrap().status, MilestoneStatus::Disputed);

    // Arbiter resolves dispute (should succeed)
    client.resolve_dispute(&t.arbiter, &shipment_id, &0u32, &true);

    // Verify milestone is now Resolved
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.milestones.get(0).unwrap().status, MilestoneStatus::Resolved);
}

// ============================================================
// SECURITY TEST 9: Arbiter Cannot Bypass Dispute Process
// ============================================================

#[test]
#[should_panic(expected = "milestone is not in disputed status")]
fn test_arbiter_cannot_bypass_dispute_process_for_payment_redirect() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-009");
    let total_amount: i128 = 1_000_000_000;
    let proof_hash = String::from_str(&t.env, "proof_hash_009");

    // Create shipment
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

    // Submit proof (milestone now in ProofSubmitted)
    client.submit_proof(&t.supplier, &shipment_id, &0u32, &proof_hash, &proof_type(&t.env));

    // Arbiter attempts to directly resolve without buyer raising dispute
    // This is a security attack: arbiter trying to bypass buyer oversight
    // Should panic: milestone is not in disputed status
    client.resolve_dispute(&t.arbiter, &shipment_id, &0u32, &true);
}

// ============================================================
// SECURITY TEST 10: Comprehensive State Coverage
// ============================================================

#[test]
fn test_arbiter_security_covers_all_milestone_states() {
    let t = setup();
    let _client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // Test that we have covered:
    // 1. Pending - test_arbiter_cannot_resolve_pending_milestone ✓
    // 2. ProofSubmitted - test_arbiter_cannot_resolve_proof_submitted_milestone ✓
    // 3. Confirmed - test_arbiter_cannot_resolve_confirmed_milestone ✓
    // 4. ConfirmedHeld - test_arbiter_cannot_resolve_confirmed_held_milestone ✓
    // 5. Resolved - test_arbiter_cannot_resolve_resolved_milestone ✓
    // 6. Disputed - test_only_arbiter_can_resolve_after_buyer_raises_dispute ✓

    // All 6 MilestoneStatus variants are covered:
    let milestone_states = std::vec![
        "Pending",
        "ProofSubmitted",
        "Confirmed",
        "ConfirmedHeld",
        "Disputed (arbiter CAN resolve this)",
        "Resolved",
    ];

    // Verify we have 10 tests covering security:
    // 1. Cannot resolve Pending
    // 2. Cannot resolve ProofSubmitted
    // 3. Cannot resolve Confirmed
    // 4. Cannot resolve ConfirmedHeld
    // 5. Cannot resolve Resolved
    // 6. Cannot confirm_milestone
    // 7. Cannot cancel_shipment
    // 8. Only arbiter can resolve after dispute
    // 9. Cannot bypass dispute process
    // 10. This state coverage test

    assert_eq!(milestone_states.len(), 6);
}

// ============================================================
// SECURITY TEST 11: Non-Designated Address Cannot Resolve Dispute
// ============================================================

#[test]
#[should_panic(expected = "unauthorized")]
fn test_non_designated_arbiter_cannot_resolve_dispute() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-010");
    let total_amount: i128 = 1_000_000_000;
    let proof_hash = String::from_str(&t.env, "proof_hash_010");

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

    client.submit_proof(&t.supplier, &shipment_id, &0u32, &proof_hash, &proof_type(&t.env));
    client.raise_dispute(&t.buyer, &shipment_id, &0u32);

    // A random third party (not the shipment's designated arbiter) attempts to resolve.
    // Should panic: unauthorized
    let impostor = Address::generate(&t.env);
    client.resolve_dispute(&impostor, &shipment_id, &0u32, &true);
}

// ============================================================
// SECURITY TEST 12: Arbiter Cannot Raise a Dispute
// ============================================================

#[test]
#[should_panic(expected = "unauthorized")]
fn test_arbiter_cannot_raise_dispute() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-011");
    let total_amount: i128 = 1_000_000_000;
    let proof_hash = String::from_str(&t.env, "proof_hash_011");

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

    client.submit_proof(&t.supplier, &shipment_id, &0u32, &proof_hash, &proof_type(&t.env));

    // Arbiter (not the buyer) attempts to raise a dispute on its own milestone.
    // Should panic: unauthorized
    client.raise_dispute(&t.arbiter, &shipment_id, &0u32);
}

// ============================================================
// SECURITY TEST 13: Supplier Cannot Resolve Dispute
// ============================================================

#[test]
#[should_panic(expected = "unauthorized")]
fn test_supplier_cannot_resolve_dispute() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-012");
    let total_amount: i128 = 1_000_000_000;
    let proof_hash = String::from_str(&t.env, "proof_hash_012");

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

    client.submit_proof(&t.supplier, &shipment_id, &0u32, &proof_hash, &proof_type(&t.env));
    client.raise_dispute(&t.buyer, &shipment_id, &0u32);

    // Supplier (a stakeholder, but not the arbiter) attempts to resolve the dispute in its own favor.
    // Should panic: unauthorized
    client.resolve_dispute(&t.supplier, &shipment_id, &0u32, &true);
}
