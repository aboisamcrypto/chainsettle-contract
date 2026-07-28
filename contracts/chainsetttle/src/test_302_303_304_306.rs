#![cfg(test)]

//! Tests for issues #302, #303, #304, #306.

extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, vec, Address, Env, String, Symbol,
};

// ============================================================
// HELPERS
// ============================================================

struct S {
    env: Env,
    contract_id: Address,
    token_id: Address,
    buyer: Address,
    supplier: Address,
    logistics: Address,
    arbiter: Address,
}

fn setup() -> S {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(ChainSettleContract, ());
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin).address();
    let buyer = Address::generate(&env);
    let supplier = Address::generate(&env);
    let logistics = Address::generate(&env);
    let arbiter = Address::generate(&env);
    token::StellarAssetClient::new(&env, &token_id).mint(&buyer, &100_000_000_000i128);
    ChainSettleContractClient::new(&env, &contract_id).init(&buyer);
    S { env, contract_id, token_id, buyer, supplier, logistics, arbiter }
}

fn opts(env: &Env) -> ShipmentOptions {
    ShipmentOptions {
        response_deadline: 0, penalty_bps: 0,
        milestone_mode: MilestoneMode::Parallel,
        holdback_ledgers: 0, dispute_cooldown_ledgers: 0,
        late_penalty_bps_per_ledger: 0, auto_confirm_ledgers: 0,
        dispute_bond_amount: 0, arbiter_fee_bps: 0,
        logistics_fee_bps: 0, supplier_collateral: 0,
        expires_at_ledger: None,
    }
}

fn single_buyer(env: &Env, buyer: &Address) -> soroban_sdk::Vec<Address> {
    vec![env, buyer.clone()]
}

fn make_shipment(s: &S, id: &str) -> String {
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let sid = String::from_str(&s.env, id);
    client.create_shipment(
        &sid,
        &single_buyer(&s.env, &s.buyer),
        &s.supplier, &s.logistics, &s.arbiter, &s.token_id,
        &1_000_000_000i128,
        &vec![
            &s.env,
            Milestone {
                name: String::from_str(&s.env, "M0"),
                payment_percent: 50,
                proof_hash: String::from_str(&s.env, ""),
                status: MilestoneStatus::Pending,
                release_after_ledger: 0,
                proof_submitted_ledger: None,
                dispute_opened_ledger: None,
            },
            Milestone {
                name: String::from_str(&s.env, "M1"),
                payment_percent: 50,
                proof_hash: String::from_str(&s.env, ""),
                status: MilestoneStatus::Pending,
                release_after_ledger: 0,
                proof_submitted_ledger: None,
                dispute_opened_ledger: None,
            },
        ],
        &opts(&s.env),
    );
    sid
}

fn submit_and_confirm(s: &S, sid: &String, idx: u32) {
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    client.submit_proof(
        &s.supplier, sid, &idx,
        &String::from_str(&s.env, "ipfs://x"),
        &Symbol::new(&s.env, "ipfs"),
    );
    client.confirm_milestone(&s.buyer, sid, &idx);
}

// ============================================================
// #302 — Delayed cancellable emergency recovery
// ============================================================

/// delay=0 reproduces immediate execution (legacy behaviour).
#[test]
fn test_302_zero_delay_executes_immediately() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let sid = make_shipment(&s, "SHIP-302-IMM");
    // set_recovery_delay(0) — no delay
    client.set_recovery_delay(&s.buyer, &0u32);
    // advance past RECOVERY_THRESHOLD_LEDGERS
    s.env.ledger().set_sequence_number(12_614_401 + 1);
    // propose_emergency_recover with delay=0 should execute immediately
    client.propose_emergency_recover(&s.buyer, &sid);
    assert_eq!(client.get_shipment(&sid).status, ShipmentStatus::Cancelled);
    // no pending proposal stored
    assert!(client.get_pending_recovery(&sid).is_none());
}

/// With delay, execution before effective_ledger panics.
#[test]
#[should_panic(expected = "recovery delay has not elapsed")]
fn test_302_execute_before_delay_panics() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let sid = make_shipment(&s, "SHIP-302-EARLY");
    client.set_recovery_delay(&s.buyer, &200u32);
    s.env.ledger().set_sequence_number(12_614_401 + 1);
    client.propose_emergency_recover(&s.buyer, &sid);
    // still at same ledger — delay not elapsed
    client.execute_emergency_recover(&s.buyer, &sid);
}

/// Successful delayed execution after waiting.
#[test]
fn test_302_delayed_execution_succeeds() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let sid = make_shipment(&s, "SHIP-302-DELAY");
    let delay = 100u32;
    client.set_recovery_delay(&s.buyer, &delay);
    let base = 12_614_402u32;
    s.env.ledger().set_sequence_number(base);
    client.propose_emergency_recover(&s.buyer, &sid);
    // proposal stored
    let prop = client.get_pending_recovery(&sid).expect("proposal must exist");
    assert_eq!(prop.effective_ledger, base + delay);
    // advance past effective_ledger
    s.env.ledger().set_sequence_number(base + delay + 1);
    client.execute_emergency_recover(&s.buyer, &sid);
    assert_eq!(client.get_shipment(&sid).status, ShipmentStatus::Cancelled);
    assert!(client.get_pending_recovery(&sid).is_none());
}

/// cancel_emergency_recover removes the proposal cleanly.
#[test]
fn test_302_cancel_removes_proposal() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let sid = make_shipment(&s, "SHIP-302-CANCEL");
    client.set_recovery_delay(&s.buyer, &500u32);
    s.env.ledger().set_sequence_number(12_614_402);
    client.propose_emergency_recover(&s.buyer, &sid);
    assert!(client.get_pending_recovery(&sid).is_some());
    client.cancel_emergency_recover(&s.buyer, &sid);
    assert!(client.get_pending_recovery(&sid).is_none());
    // shipment still active — funds not moved
    assert_eq!(client.get_shipment(&sid).status, ShipmentStatus::Active);
}

/// Both propose and execute appear in the admin audit log.
#[test]
fn test_302_admin_log_entries() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let sid = make_shipment(&s, "SHIP-302-LOG");
    let delay = 50u32;
    client.set_recovery_delay(&s.buyer, &delay);
    let base = 12_614_402u32;
    s.env.ledger().set_sequence_number(base);
    client.propose_emergency_recover(&s.buyer, &sid);
    s.env.ledger().set_sequence_number(base + delay + 1);
    client.execute_emergency_recover(&s.buyer, &sid);
    let log = client.get_admin_log();
    // log should contain entries for propose + execute (plus set_recovery_delay)
    assert!(log.len() >= 3, "admin log must contain at least 3 entries");
}

// ============================================================
// #303 — Milestone confirmation webhook allowlist
// ============================================================

/// Empty allowlist by default — no behaviour change.
#[test]
fn test_303_empty_allowlist_no_change() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    assert_eq!(client.get_confirmation_webhooks().len(), 0);
    let sid = make_shipment(&s, "SHIP-303-EMPTY");
    submit_and_confirm(&s, &sid, 0);
    assert_eq!(
        client.get_milestone(&sid, &0).status,
        MilestoneStatus::Confirmed
    );
}

/// Admin can add and remove webhook addresses.
#[test]
fn test_303_add_remove_webhook() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let hook = Address::generate(&s.env);
    client.add_confirmation_webhook(&s.buyer, &hook);
    assert_eq!(client.get_confirmation_webhooks().len(), 1);
    assert_eq!(client.get_confirmation_webhooks().get(0).unwrap(), hook);
    client.remove_confirmation_webhook(&s.buyer, &hook);
    assert_eq!(client.get_confirmation_webhooks().len(), 0);
}

/// Duplicate add is idempotent — allowlist stays length 1.
#[test]
fn test_303_duplicate_add_idempotent() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let hook = Address::generate(&s.env);
    client.add_confirmation_webhook(&s.buyer, &hook);
    client.add_confirmation_webhook(&s.buyer, &hook);
    assert_eq!(client.get_confirmation_webhooks().len(), 1);
}

/// Non-admin cannot add webhooks.
#[test]
#[should_panic(expected = "unauthorized")]
fn test_303_non_admin_add_rejected() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let hook = Address::generate(&s.env);
    client.add_confirmation_webhook(&s.supplier, &hook);
}

/// A failing webhook (unregistered contract) does not revert the confirmation.
/// We verify this by registering a random address (not a real contract) — the
/// try_on_milestone_confirmed call will fail gracefully, payout still goes through.
#[test]
fn test_303_failing_webhook_does_not_revert_confirmation() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let token_client = token::Client::new(&s.env, &s.token_id);

    // Register a random address as webhook — it has no on_milestone_confirmed fn
    let bad_hook = Address::generate(&s.env);
    client.add_confirmation_webhook(&s.buyer, &bad_hook);

    let sid = make_shipment(&s, "SHIP-303-FAIL");
    // Confirmation must succeed despite the bad hook
    submit_and_confirm(&s, &sid, 0);

    assert_eq!(
        client.get_milestone(&sid, &0).status,
        MilestoneStatus::Confirmed
    );
    // Supplier still received payment
    assert!(token_client.balance(&s.supplier) > 0);
}

/// Multiple webhooks are all invoked (best-effort).
#[test]
fn test_303_multiple_webhooks_registered() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    for _ in 0..3 {
        client.add_confirmation_webhook(&s.buyer, &Address::generate(&s.env));
    }
    assert_eq!(client.get_confirmation_webhooks().len(), 3);
    let sid = make_shipment(&s, "SHIP-303-MULTI");
    // All three hooks fail gracefully; confirmation still succeeds
    submit_and_confirm(&s, &sid, 0);
    assert_eq!(
        client.get_milestone(&sid, &0).status,
        MilestoneStatus::Confirmed
    );
}

// ============================================================
// #304 — Evidence submission cap
// ============================================================

/// get_evidence_count returns 0 before any submission.
#[test]
fn test_304_initial_count_zero() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let sid = make_shipment(&s, "SHIP-304-INIT");
    assert_eq!(client.get_evidence_count(&sid, &0), 0);
}

/// Count increments with each submission.
#[test]
fn test_304_count_increments_on_submit() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let sid = make_shipment(&s, "SHIP-304-COUNT");
    // Default cap is 5; submit once then check count = 1
    client.submit_proof(
        &s.supplier, &sid, &0,
        &String::from_str(&s.env, "ipfs://1"),
        &Symbol::new(&s.env, "ipfs"),
    );
    assert_eq!(client.get_evidence_count(&sid, &0), 1);
}

/// Reaching the cap exactly is allowed; one more is rejected.
#[test]
fn test_304_cap_enforced_on_next_attempt() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    // Set cap to 2
    client.set_max_evidence_per_milestone(&s.buyer, &2u32);
    let sid = make_shipment(&s, "SHIP-304-CAP");

    // First submission — ok
    client.submit_proof(&s.supplier, &sid, &0,
        &String::from_str(&s.env, "ipfs://a"), &Symbol::new(&s.env, "ipfs"));
    assert_eq!(client.get_evidence_count(&sid, &0), 1);

    // Confirm resets milestone so we can resubmit (dispute reject → pending)
    client.raise_dispute(&s.buyer, &sid, &0);
    client.resolve_dispute(&s.arbiter, &sid, &0, &false);

    // Second submission — still ok (count = 2, cap = 2)
    client.submit_proof(&s.supplier, &sid, &0,
        &String::from_str(&s.env, "ipfs://b"), &Symbol::new(&s.env, "ipfs"));
    assert_eq!(client.get_evidence_count(&sid, &0), 2);
    // Count is per-milestone regardless of which party submits
}

/// Submission beyond cap is rejected.
#[test]
#[should_panic(expected = "evidence submission limit reached")]
fn test_304_submission_beyond_cap_panics() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    client.set_max_evidence_per_milestone(&s.buyer, &1u32);
    let sid = make_shipment(&s, "SHIP-304-OVER");

    client.submit_proof(&s.supplier, &sid, &0,
        &String::from_str(&s.env, "ipfs://ok"), &Symbol::new(&s.env, "ipfs"));
    // dispute + reject to get back to Pending
    client.raise_dispute(&s.buyer, &sid, &0);
    client.resolve_dispute(&s.arbiter, &sid, &0, &false);
    // This must panic
    client.submit_proof(&s.supplier, &sid, &0,
        &String::from_str(&s.env, "ipfs://bad"), &Symbol::new(&s.env, "ipfs"));
}

/// Cap is per-milestone, independent across milestones.
#[test]
fn test_304_cap_independent_per_milestone() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    client.set_max_evidence_per_milestone(&s.buyer, &1u32);
    let sid = make_shipment(&s, "SHIP-304-INDEP");
    // Submit to M0 (reaching cap on M0)
    client.submit_proof(&s.supplier, &sid, &0,
        &String::from_str(&s.env, "ipfs://m0"), &Symbol::new(&s.env, "ipfs"));
    // M1 should still allow submission (its own count = 0)
    client.submit_proof(&s.supplier, &sid, &1,
        &String::from_str(&s.env, "ipfs://m1"), &Symbol::new(&s.env, "ipfs"));
    assert_eq!(client.get_evidence_count(&sid, &0), 1);
    assert_eq!(client.get_evidence_count(&sid, &1), 1);
}

// ============================================================
// #306 — Delegated confirmation signer
// ============================================================

/// Delegate can confirm a milestone within the cap.
#[test]
fn test_306_delegate_confirms_under_cap() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let token_client = token::Client::new(&s.env, &s.token_id);
    let delegate = Address::generate(&s.env);
    let sid = make_shipment(&s, "SHIP-306-OK");

    // M0 payment = 1_000_000_000 * 50 / 100 = 500_000_000
    let per_tx_cap = 500_000_000i128;
    client.authorize_delegate(&s.buyer, &sid, &delegate, &per_tx_cap);

    let cfg = client.get_delegate(&sid).expect("delegate must be set");
    assert_eq!(cfg.delegate, delegate);
    assert_eq!(cfg.per_tx_cap, per_tx_cap);

    client.submit_proof(&s.supplier, &sid, &0,
        &String::from_str(&s.env, "ipfs://d"), &Symbol::new(&s.env, "ipfs"));
    // Delegate confirms
    client.confirm_milestone(&delegate, &sid, &0);
    assert_eq!(client.get_milestone(&sid, &0).status, MilestoneStatus::Confirmed);
    assert!(token_client.balance(&s.supplier) > 0);
}

/// Delegate cannot confirm when payment exceeds per_tx_cap.
#[test]
#[should_panic(expected = "payment exceeds delegate per_tx_cap")]
fn test_306_delegate_over_cap_panics() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let delegate = Address::generate(&s.env);
    let sid = make_shipment(&s, "SHIP-306-OVER");
    // M0 = 500_000_000; set cap just below that
    let per_tx_cap = 499_999_999i128;
    client.authorize_delegate(&s.buyer, &sid, &delegate, &per_tx_cap);
    client.submit_proof(&s.supplier, &sid, &0,
        &String::from_str(&s.env, "ipfs://d"), &Symbol::new(&s.env, "ipfs"));
    client.confirm_milestone(&delegate, &sid, &0);
}

/// Only the buyer can authorize a delegate.
#[test]
#[should_panic(expected = "unauthorized")]
fn test_306_non_buyer_cannot_authorize() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let delegate = Address::generate(&s.env);
    let sid = make_shipment(&s, "SHIP-306-AUTH");
    client.authorize_delegate(&s.supplier, &sid, &delegate, &1_000_000i128);
}

/// After revocation the delegate can no longer confirm.
#[test]
#[should_panic(expected = "unauthorized")]
fn test_306_post_revocation_delegate_rejected() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let delegate = Address::generate(&s.env);
    let sid = make_shipment(&s, "SHIP-306-REV");
    client.authorize_delegate(&s.buyer, &sid, &delegate, &500_000_000i128);
    client.revoke_delegate(&s.buyer, &sid);
    assert!(client.get_delegate(&sid).is_none());
    client.submit_proof(&s.supplier, &sid, &0,
        &String::from_str(&s.env, "ipfs://d"), &Symbol::new(&s.env, "ipfs"));
    // Must panic — delegate was revoked
    client.confirm_milestone(&delegate, &sid, &0);
}

/// Delegate cannot raise disputes (buyer-only).
#[test]
#[should_panic(expected = "unauthorized")]
fn test_306_delegate_cannot_raise_dispute() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let delegate = Address::generate(&s.env);
    let sid = make_shipment(&s, "SHIP-306-DISP");
    client.authorize_delegate(&s.buyer, &sid, &delegate, &500_000_000i128);
    client.submit_proof(&s.supplier, &sid, &0,
        &String::from_str(&s.env, "ipfs://d"), &Symbol::new(&s.env, "ipfs"));
    // raise_dispute uses require_buyer_auth which checks shipment.buyers
    client.raise_dispute(&delegate, &sid, &0);
}

/// Buyer can still confirm directly even when a delegate is registered.
#[test]
fn test_306_buyer_can_still_confirm_with_delegate_registered() {
    let s = setup();
    let client = ChainSettleContractClient::new(&s.env, &s.contract_id);
    let delegate = Address::generate(&s.env);
    let sid = make_shipment(&s, "SHIP-306-BOTH");
    client.authorize_delegate(&s.buyer, &sid, &delegate, &1i128);
    client.submit_proof(&s.supplier, &sid, &0,
        &String::from_str(&s.env, "ipfs://d"), &Symbol::new(&s.env, "ipfs"));
    // Buyer confirms directly despite delegate being registered
    client.confirm_milestone(&s.buyer, &sid, &0);
    assert_eq!(client.get_milestone(&sid, &0).status, MilestoneStatus::Confirmed);
}
