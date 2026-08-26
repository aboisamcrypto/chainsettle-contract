#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec, TestSetup};
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{token, vec, Address, BytesN, Env, String, Symbol, TryFromVal};

fn sid(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

fn ipfs(env: &Env) -> Symbol {
    Symbol::new(env, "ipfs")
}

fn proof_hash(env: &Env) -> String {
    String::from_str(env, "QmProof")
}

fn reason_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[7u8; 32])
}

fn single_milestone_with_deadline(env: &Env, deadline_ledger: u32) -> soroban_sdk::Vec<Milestone> {
    vec![
        env,
        Milestone {
            name: sid(env, "M"),
            payment_percent: 100,
            proof_hash: sid(env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger,
            penalty_bps_per_ledger: 0,
        },
    ]
}

fn create_with_deadline(
    t: &TestSetup,
    client: &ChainSettleContractClient,
    shipment_id: &String,
    deadline_ledger: u32,
) {
    client.create_shipment(
        shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000,
        &single_milestone_with_deadline(&t.env, deadline_ledger),
        &default_options(&t.env),
    );
}

fn create_standard(t: &TestSetup, client: &ChainSettleContractClient, shipment_id: &String) {
    client.create_shipment(
        shipment_id,
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

fn only_topic(env: &Env) -> Symbol {
    let events = env.events().all();
    assert_eq!(events.len(), 1, "expected exactly one event");
    let (_id, topics, _data) = events.get(0).unwrap();
    Symbol::try_from_val(env, &topics.get(0).unwrap()).unwrap()
}

// ============================================================
// #366 — Escrow deadline warning event emitted before expiry
// ============================================================

#[test]
fn test_deadline_warning_disabled_by_default_is_noop() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    let deadline = t.env.ledger().sequence() + 100;
    create_with_deadline(&t, &client, &ship_id, deadline);

    t.env.ledger().set_sequence_number(deadline - 5);
    client.check_deadline_warning(&ship_id, &0u32);
    assert_eq!(t.env.events().all().len(), 0);
}

#[test]
fn test_deadline_warning_fires_within_window() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    let deadline = t.env.ledger().sequence() + 100;
    create_with_deadline(&t, &client, &ship_id, deadline);
    client.set_warning_lead_ledgers(&t.buyer, &20u32);

    t.env.ledger().set_sequence_number(deadline - 10);
    client.check_deadline_warning(&ship_id, &0u32);

    let topic = only_topic(&t.env);
    assert_eq!(topic, Symbol::new(&t.env, "deadline_approaching"));
}

#[test]
fn test_deadline_warning_fires_exactly_at_window_boundary() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    let deadline = t.env.ledger().sequence() + 100;
    create_with_deadline(&t, &client, &ship_id, deadline);
    client.set_warning_lead_ledgers(&t.buyer, &20u32);

    // Exactly at the start of the lead window (deadline - lead).
    t.env.ledger().set_sequence_number(deadline - 20);
    client.check_deadline_warning(&ship_id, &0u32);
    assert_eq!(t.env.events().all().len(), 1);
}

#[test]
fn test_deadline_warning_noop_before_window() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    let deadline = t.env.ledger().sequence() + 100;
    create_with_deadline(&t, &client, &ship_id, deadline);
    client.set_warning_lead_ledgers(&t.buyer, &20u32);

    // One ledger before the window opens.
    t.env.ledger().set_sequence_number(deadline - 21);
    client.check_deadline_warning(&ship_id, &0u32);
    assert_eq!(t.env.events().all().len(), 0);
}

#[test]
fn test_deadline_warning_noop_at_or_after_deadline() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    let deadline = t.env.ledger().sequence() + 100;
    create_with_deadline(&t, &client, &ship_id, deadline);
    client.set_warning_lead_ledgers(&t.buyer, &20u32);

    t.env.ledger().set_sequence_number(deadline);
    client.check_deadline_warning(&ship_id, &0u32);
    assert_eq!(t.env.events().all().len(), 0);
}

#[test]
fn test_deadline_warning_duplicate_call_suppressed() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    let deadline = t.env.ledger().sequence() + 100;
    create_with_deadline(&t, &client, &ship_id, deadline);
    client.set_warning_lead_ledgers(&t.buyer, &20u32);

    t.env.ledger().set_sequence_number(deadline - 10);
    client.check_deadline_warning(&ship_id, &0u32);
    assert_eq!(t.env.events().all().len(), 1);

    // Still within the window, but already fired — must not fire again.
    t.env.ledger().set_sequence_number(deadline - 5);
    client.check_deadline_warning(&ship_id, &0u32);
    assert_eq!(t.env.events().all().len(), 0);
}

// ============================================================
// #367 — Co-buyer joint confirmation for high-value shipments
// ============================================================

#[test]
fn test_joint_confirmation_single_buyer_confirmation_does_not_release_funds() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    client.set_joint_confirmation_threshold(&t.buyer, &500_000i128);
    client.set_co_buyer(&t.buyer, &ship_id, &t.buyer2);

    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &proof_hash(&t.env),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);

    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::ProofSubmitted);

    let token_client = token::Client::new(&t.env, &t.token_id);
    assert_eq!(token_client.balance(&t.supplier), 0);

    let status = client.get_joint_confirmation_status(&ship_id, &0u32);
    assert!(status.buyer_confirmed);
    assert!(!status.co_buyer_confirmed);
}

#[test]
fn test_joint_confirmation_both_confirm_releases_payment() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    client.set_joint_confirmation_threshold(&t.buyer, &500_000i128);
    client.set_co_buyer(&t.buyer, &ship_id, &t.buyer2);

    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &proof_hash(&t.env),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);
    client.confirm_milestone(&t.buyer2, &ship_id, &0u32);

    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::Confirmed);

    let token_client = token::Client::new(&t.env, &t.token_id);
    // Milestone 0 is 25% of the 1_000_000 shipment.
    assert_eq!(token_client.balance(&t.supplier), 250_000);
}

#[test]
fn test_joint_confirmation_co_buyer_can_confirm_first() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    client.set_joint_confirmation_threshold(&t.buyer, &500_000i128);
    client.set_co_buyer(&t.buyer, &ship_id, &t.buyer2);

    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &proof_hash(&t.env),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer2, &ship_id, &0u32);
    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::ProofSubmitted);

    client.confirm_milestone(&t.buyer, &ship_id, &0u32);
    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::Confirmed);
}

#[test]
fn test_joint_confirmation_below_threshold_unaffected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    // Threshold above the shipment's total_amount — joint confirmation must not apply.
    client.set_joint_confirmation_threshold(&t.buyer, &10_000_000i128);
    client.set_co_buyer(&t.buyer, &ship_id, &t.buyer2);

    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &proof_hash(&t.env),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);

    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::Confirmed);
    let token_client = token::Client::new(&t.env, &t.token_id);
    assert_eq!(token_client.balance(&t.supplier), 250_000);
}

#[test]
fn test_joint_confirmation_no_co_buyer_designated_unaffected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    // Above threshold, but no co-buyer was ever designated.
    client.set_joint_confirmation_threshold(&t.buyer, &500_000i128);

    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &proof_hash(&t.env),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);

    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::Confirmed);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_joint_confirmation_unrelated_caller_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    client.set_joint_confirmation_threshold(&t.buyer, &500_000i128);
    client.set_co_buyer(&t.buyer, &ship_id, &t.buyer2);

    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &proof_hash(&t.env),
        &ipfs(&t.env),
    );
    // Logistics is neither the buyer, the co-buyer, nor a registered delegate.
    client.confirm_milestone(&t.logistics, &ship_id, &0u32);
}

#[test]
#[should_panic(expected = "co-buyer is already set")]
fn test_set_co_buyer_immutable_once_set() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    client.set_co_buyer(&t.buyer, &ship_id, &t.buyer2);
    client.set_co_buyer(&t.buyer, &ship_id, &t.buyer2);
}

#[test]
#[should_panic(expected = "co-buyer must be set before the shipment progresses")]
fn test_set_co_buyer_rejected_after_progress() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &proof_hash(&t.env),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);

    client.set_co_buyer(&t.buyer, &ship_id, &t.buyer2);
}

// ============================================================
// #368 — Compliance hold
// ============================================================

#[test]
#[should_panic(expected = "shipment is on compliance hold pending review")]
fn test_compliance_hold_blocks_confirm() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);
    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &proof_hash(&t.env),
        &ipfs(&t.env),
    );

    client.set_compliance_hold(&t.buyer, &ship_id, &reason_hash(&t.env));
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);
}

#[test]
#[should_panic(expected = "shipment is on compliance hold pending review")]
fn test_compliance_hold_blocks_dispute() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    client.set_compliance_hold(&t.buyer, &ship_id, &reason_hash(&t.env));
    client.raise_dispute(&t.buyer, &ship_id, &0u32);
}

#[test]
#[should_panic(expected = "shipment is on compliance hold pending review")]
fn test_compliance_hold_blocks_cancel() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    client.set_compliance_hold(&t.buyer, &ship_id, &reason_hash(&t.env));
    client.cancel_shipment(&t.buyer, &ship_id);
}

#[test]
fn test_compliance_hold_other_shipments_unaffected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let held_id = sid(&t.env, "held");
    let free_id = sid(&t.env, "free");
    create_standard(&t, &client, &held_id);
    create_standard(&t, &client, &free_id);

    client.set_compliance_hold(&t.buyer, &held_id, &reason_hash(&t.env));
    assert!(client.is_on_compliance_hold(&held_id));
    assert!(!client.is_on_compliance_hold(&free_id));

    client.submit_proof(
        &t.supplier,
        &free_id,
        &0u32,
        &proof_hash(&t.env),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer, &free_id, &0u32);
    let milestone = client.get_milestone(&free_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::Confirmed);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_compliance_hold_only_admin_can_set() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    client.set_compliance_hold(&t.supplier, &ship_id, &reason_hash(&t.env));
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_compliance_hold_only_admin_can_clear() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    client.set_compliance_hold(&t.buyer, &ship_id, &reason_hash(&t.env));
    client.clear_compliance_hold(&t.supplier, &ship_id);
}

#[test]
fn test_compliance_hold_clear_allows_operations_again() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);
    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &proof_hash(&t.env),
        &ipfs(&t.env),
    );

    client.set_compliance_hold(&t.buyer, &ship_id, &reason_hash(&t.env));
    client.clear_compliance_hold(&t.buyer, &ship_id);
    assert!(!client.is_on_compliance_hold(&ship_id));

    client.confirm_milestone(&t.buyer, &ship_id, &0u32);
    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::Confirmed);
}

// ============================================================
// #369 — Dispute appeal
// ============================================================

fn open_and_resolve(
    t: &TestSetup,
    client: &ChainSettleContractClient,
    ship_id: &String,
    approve: bool,
) {
    client.submit_proof(
        &t.supplier,
        ship_id,
        &0u32,
        &proof_hash(&t.env),
        &ipfs(&t.env),
    );
    client.raise_dispute(&t.buyer, ship_id, &0u32);
    client.resolve_dispute(&t.arbiter, ship_id, &0u32, &approve);
}

#[test]
fn test_appeal_dispute_reassigns_distinct_arbiter_and_allows_final_resolution() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    let arbiter2 = Address::generate(&t.env);
    client.add_arbiter_to_pool(&t.buyer, &arbiter2);
    client.set_appeal_window_ledgers(&t.buyer, &50u32);

    open_and_resolve(&t, &client, &ship_id, true);
    let resolved_milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(resolved_milestone.status, MilestoneStatus::Resolved);

    client.appeal_dispute(&t.buyer, &ship_id, &0u32);

    let shipment = client.get_shipment(&ship_id);
    assert_eq!(shipment.arbiter, arbiter2);
    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::Disputed);

    // The reassigned arbiter issues the final, second resolution.
    client.resolve_dispute(&arbiter2, &ship_id, &0u32, &false);
    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::Pending);
}

#[test]
#[should_panic(expected = "dispute has already been appealed")]
fn test_appeal_dispute_double_appeal_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    let arbiter2 = Address::generate(&t.env);
    client.add_arbiter_to_pool(&t.buyer, &arbiter2);
    client.set_appeal_window_ledgers(&t.buyer, &50u32);

    open_and_resolve(&t, &client, &ship_id, true);
    client.appeal_dispute(&t.buyer, &ship_id, &0u32);
    // Second appeal on the same dispute cycle must be rejected.
    client.appeal_dispute(&t.supplier, &ship_id, &0u32);
}

#[test]
fn test_appeal_dispute_window_expired_rejected_and_resolution_stands() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    let arbiter2 = Address::generate(&t.env);
    client.add_arbiter_to_pool(&t.buyer, &arbiter2);
    client.set_appeal_window_ledgers(&t.buyer, &50u32);

    open_and_resolve(&t, &client, &ship_id, true);
    let resolved_at = t.env.ledger().sequence();

    t.env.ledger().set_sequence_number(resolved_at + 51);

    let result = client.try_appeal_dispute(&t.buyer, &ship_id, &0u32);
    assert!(result.is_err());

    // Original resolution must stand untouched.
    let shipment = client.get_shipment(&ship_id);
    assert_eq!(shipment.arbiter, t.arbiter);
    let milestone = client.get_milestone(&ship_id, &0u32);
    assert_eq!(milestone.status, MilestoneStatus::Resolved);
}

#[test]
#[should_panic(expected = "appeals are not enabled")]
fn test_appeal_dispute_disabled_by_default() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    open_and_resolve(&t, &client, &ship_id, true);
    client.appeal_dispute(&t.buyer, &ship_id, &0u32);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_appeal_dispute_unrelated_caller_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "s1");
    create_standard(&t, &client, &ship_id);

    let arbiter2 = Address::generate(&t.env);
    client.add_arbiter_to_pool(&t.buyer, &arbiter2);
    client.set_appeal_window_ledgers(&t.buyer, &50u32);

    open_and_resolve(&t, &client, &ship_id, true);
    client.appeal_dispute(&t.logistics, &ship_id, &0u32);
}
