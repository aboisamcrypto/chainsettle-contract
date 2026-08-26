#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Address as _, vec, Address, Env, String, Symbol};

fn sid(env: &Env, id: &str) -> String {
    String::from_str(env, id)
}

fn ipfs(env: &Env) -> Symbol {
    Symbol::new(env, "ipfs")
}

// ─────────────────────────────────────────────────────────────────
// Shared helper: create a shipment with an arbiter panel
// ─────────────────────────────────────────────────────────────────

fn panel_options(env: &Env, panel: soroban_sdk::Vec<Address>) -> ShipmentOptions {
    let mut opts = default_options(env);
    opts.arbiter_panel = panel;
    opts
}

fn create_panel_shipment(
    client: &ChainSettleContractClient,
    env: &Env,
    shipment_id: &String,
    buyer: &Address,
    supplier: &Address,
    logistics: &Address,
    arbiter: &Address,
    token_id: &Address,
    panel: soroban_sdk::Vec<Address>,
) {
    client.create_shipment(
        shipment_id,
        &single_buyer_vec(env, buyer),
        supplier,
        logistics,
        arbiter,
        token_id,
        &1_000_000i128,
        &build_milestones(env),
        &panel_options(env, panel),
    );
}

// ═══════════════════════════════════════════════════════════════════
// FEATURE A – ARBITER PANEL (N-of-M dispute voting)
// ═══════════════════════════════════════════════════════════════════

// ─── Panel creation ────────────────────────────────────────────────

#[test]
fn test_panel_stored_on_creation() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let arbiter2 = Address::generate(&t.env);
    let arbiter3 = Address::generate(&t.env);
    let panel = vec![
        &t.env,
        t.arbiter.clone(),
        arbiter2.clone(),
        arbiter3.clone(),
    ];
    let ship_id = sid(&t.env, "panel_create");

    create_panel_shipment(
        &client,
        &t.env,
        &ship_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        panel.clone(),
    );

    let stored = client.get_arbiter_panel(&ship_id);
    assert_eq!(stored.len(), 3, "panel should have 3 members");
}

#[test]
fn test_single_arbiter_shipment_unaffected_by_panel_feature() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "no_panel");

    // default_options has empty arbiter_panel
    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    assert_eq!(
        client.get_arbiter_panel(&ship_id).len(),
        0,
        "no panel for single-arbiter shipment"
    );
}

#[test]
#[should_panic(expected = "arbiter panel must have at least 3 members")]
fn test_panel_too_small_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let arbiter2 = Address::generate(&t.env);
    // Only 2 members – must panic.
    let panel = vec![&t.env, t.arbiter.clone(), arbiter2.clone()];
    create_panel_shipment(
        &client,
        &t.env,
        &sid(&t.env, "small_panel"),
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        panel,
    );
}

// ─── Voting and majority resolution ────────────────────────────────

#[test]
fn test_panel_2_of_3_majority_approve_resolves_dispute() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let arbiter2 = Address::generate(&t.env);
    let arbiter3 = Address::generate(&t.env);
    let panel = vec![
        &t.env,
        t.arbiter.clone(),
        arbiter2.clone(),
        arbiter3.clone(),
    ];
    let ship_id = sid(&t.env, "panel_21_approve");

    create_panel_shipment(
        &client,
        &t.env,
        &ship_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        panel,
    );

    // Submit proof and raise dispute.
    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h0"),
        &ipfs(&t.env),
    );
    client.raise_dispute(&t.buyer, &ship_id, &0u32);

    // First vote: approve. No majority yet (1/3).
    client.cast_dispute_vote(&t.arbiter, &ship_id, &0u32, &true);
    let m = client.get_milestone(&ship_id, &0u32);
    assert_eq!(
        m.status,
        MilestoneStatus::Disputed,
        "still disputed after 1 vote"
    );

    // Second vote: approve. Majority reached (2/3) → auto-resolves.
    client.cast_dispute_vote(&arbiter2, &ship_id, &0u32, &true);
    let m = client.get_milestone(&ship_id, &0u32);
    assert_eq!(
        m.status,
        MilestoneStatus::Resolved,
        "should be resolved after 2-of-3 approve"
    );
}

#[test]
fn test_panel_3_of_5_majority_reject_resets_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let arb = (0..4)
        .map(|_| Address::generate(&t.env))
        .collect::<std::vec::Vec<_>>();
    let panel = vec![
        &t.env,
        t.arbiter.clone(),
        arb[0].clone(),
        arb[1].clone(),
        arb[2].clone(),
        arb[3].clone(),
    ];
    let ship_id = sid(&t.env, "panel_35_reject");

    create_panel_shipment(
        &client,
        &t.env,
        &ship_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        panel,
    );

    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h0"),
        &ipfs(&t.env),
    );
    client.raise_dispute(&t.buyer, &ship_id, &0u32);

    // 3 reject votes → majority (3 > 5/2 = 2) → resolves as buyer win (Pending).
    client.cast_dispute_vote(&t.arbiter, &ship_id, &0u32, &false);
    client.cast_dispute_vote(&arb[0], &ship_id, &0u32, &false);
    client.cast_dispute_vote(&arb[1], &ship_id, &0u32, &false);

    let m = client.get_milestone(&ship_id, &0u32);
    assert_eq!(
        m.status,
        MilestoneStatus::Pending,
        "full dispute reject should reset to Pending"
    );
}

// ─── Duplicate vote and non-member rejection ───────────────────────

#[test]
#[should_panic(expected = "AlreadyVoted")]
fn test_panel_duplicate_vote_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let arbiter2 = Address::generate(&t.env);
    let arbiter3 = Address::generate(&t.env);
    let panel = vec![
        &t.env,
        t.arbiter.clone(),
        arbiter2.clone(),
        arbiter3.clone(),
    ];
    let ship_id = sid(&t.env, "panel_dup_vote");

    create_panel_shipment(
        &client,
        &t.env,
        &ship_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        panel,
    );
    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h0"),
        &ipfs(&t.env),
    );
    client.raise_dispute(&t.buyer, &ship_id, &0u32);

    client.cast_dispute_vote(&t.arbiter, &ship_id, &0u32, &true);
    // Second vote from same arbiter must panic.
    client.cast_dispute_vote(&t.arbiter, &ship_id, &0u32, &true);
}

#[test]
#[should_panic(expected = "NotPanelMember")]
fn test_non_panel_member_vote_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let arbiter2 = Address::generate(&t.env);
    let arbiter3 = Address::generate(&t.env);
    let outsider = Address::generate(&t.env);
    let panel = vec![
        &t.env,
        t.arbiter.clone(),
        arbiter2.clone(),
        arbiter3.clone(),
    ];
    let ship_id = sid(&t.env, "panel_outsider");

    create_panel_shipment(
        &client,
        &t.env,
        &ship_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        panel,
    );
    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h0"),
        &ipfs(&t.env),
    );
    client.raise_dispute(&t.buyer, &ship_id, &0u32);

    // Outsider is not in the panel → must panic.
    client.cast_dispute_vote(&outsider, &ship_id, &0u32, &true);
}

#[test]
#[should_panic(expected = "DisputeAlreadyResolved")]
fn test_vote_after_resolution_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let arbiter2 = Address::generate(&t.env);
    let arbiter3 = Address::generate(&t.env);
    let panel = vec![
        &t.env,
        t.arbiter.clone(),
        arbiter2.clone(),
        arbiter3.clone(),
    ];
    let ship_id = sid(&t.env, "panel_post_resolve");

    create_panel_shipment(
        &client,
        &t.env,
        &ship_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        panel,
    );
    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h0"),
        &ipfs(&t.env),
    );
    client.raise_dispute(&t.buyer, &ship_id, &0u32);

    // 2-of-3 approve → resolves.
    client.cast_dispute_vote(&t.arbiter, &ship_id, &0u32, &true);
    client.cast_dispute_vote(&arbiter2, &ship_id, &0u32, &true);

    // Third vote after resolution must panic.
    client.cast_dispute_vote(&arbiter3, &ship_id, &0u32, &false);
}

// ═══════════════════════════════════════════════════════════════════
// FEATURE B – SUPPLIER EXPOSURE CAP
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_exposure_cap_disabled_by_default() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    assert_eq!(
        client.get_supplier_exposure_cap(),
        0i128,
        "cap should default to 0 (disabled)"
    );
}

#[test]
fn test_exposure_increases_on_new_shipment() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.create_shipment(
        &sid(&t.env, "exp1"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &500_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    assert_eq!(client.get_supplier_exposure(&t.supplier), 500_000i128);
}

#[test]
fn test_exposure_cap_enforced_on_create_shipment() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // Set cap to 800_000.
    client.set_supplier_exposure_cap(&t.buyer, &800_000i128);

    // First shipment: 500_000 – within cap.
    client.create_shipment(
        &sid(&t.env, "cap1"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &500_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    // Second shipment would bring total to 900_000 > 800_000 → must panic.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_shipment(
            &sid(&t.env, "cap2"),
            &single_buyer_vec(&t.env, &t.buyer),
            &t.supplier,
            &t.logistics,
            &t.arbiter,
            &t.token_id,
            &400_000i128,
            &build_milestones(&t.env),
            &default_options(&t.env),
        );
    }));
    assert!(
        result.is_err(),
        "second shipment should be rejected by exposure cap"
    );
}

#[test]
fn test_exposure_decreases_after_shipment_completes() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // Single 100% milestone for easy completion.
    let single_ms = vec![
        &t.env,
        Milestone {
            name: String::from_str(&t.env, "All"),
            payment_percent: 100,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ];

    let ship_id = sid(&t.env, "exp_complete");
    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &500_000i128,
        &single_ms,
        &default_options(&t.env),
    );

    assert_eq!(client.get_supplier_exposure(&t.supplier), 500_000i128);

    // Complete the shipment.
    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h0"),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);

    assert_eq!(
        client.get_supplier_exposure(&t.supplier),
        0i128,
        "exposure should be 0 after shipment completes"
    );
}

#[test]
fn test_exposure_cap_enforced_on_top_up() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.set_supplier_exposure_cap(&t.buyer, &600_000i128);

    client.create_shipment(
        &sid(&t.env, "topup_cap"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &500_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    // Top-up of 200_000 would bring total to 700_000 > 600_000 → must panic.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.top_up_escrow(&t.buyer, &sid(&t.env, "topup_cap"), &200_000i128);
    }));
    assert!(
        result.is_err(),
        "top-up exceeding exposure cap should be rejected"
    );
}

#[test]
fn test_exposure_cap_across_multiple_concurrent_shipments() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.set_supplier_exposure_cap(&t.buyer, &1_000_000i128);

    // Two shipments: 400_000 + 400_000 = 800_000 < 1_000_000 – both succeed.
    client.create_shipment(
        &sid(&t.env, "concA"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &400_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    client.create_shipment(
        &sid(&t.env, "concB"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &400_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    assert_eq!(client.get_supplier_exposure(&t.supplier), 800_000i128);

    // Third shipment: 400_000 → total would be 1_200_000 > cap → rejected.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_shipment(
            &sid(&t.env, "concC"),
            &single_buyer_vec(&t.env, &t.buyer),
            &t.supplier,
            &t.logistics,
            &t.arbiter,
            &t.token_id,
            &400_000i128,
            &build_milestones(&t.env),
            &default_options(&t.env),
        );
    }));
    assert!(
        result.is_err(),
        "third concurrent shipment should be rejected"
    );
}

// ═══════════════════════════════════════════════════════════════════
// FEATURE C – MILESTONE PAYEES
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_milestone_payees_stored_and_retrieved() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let payee2 = Address::generate(&t.env);
    let ship_id = sid(&t.env, "payees_get");

    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    let payees = vec![
        &t.env,
        MilestonePayee {
            payee: t.supplier.clone(),
            percent: 60,
        },
        MilestonePayee {
            payee: payee2.clone(),
            percent: 40,
        },
    ];
    client.set_milestone_payees(&t.buyer, &ship_id, &0u32, &payees);

    let stored = client.get_milestone_payees(&ship_id, &0u32);
    assert_eq!(stored.len(), 2);
    assert_eq!(stored.get(0).unwrap().percent, 60);
    assert_eq!(stored.get(1).unwrap().percent, 40);
}

#[test]
#[should_panic(expected = "InvalidPayeePercentages")]
fn test_payees_must_sum_to_100() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let payee2 = Address::generate(&t.env);
    let ship_id = sid(&t.env, "payees_bad_sum");

    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    // 60 + 30 = 90 ≠ 100 → must panic.
    let payees = vec![
        &t.env,
        MilestonePayee {
            payee: t.supplier.clone(),
            percent: 60,
        },
        MilestonePayee {
            payee: payee2.clone(),
            percent: 30,
        },
    ];
    client.set_milestone_payees(&t.buyer, &ship_id, &0u32, &payees);
}

#[test]
#[should_panic(expected = "MilestoneNotPending")]
fn test_payees_not_configurable_after_proof_submitted() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "payees_late");

    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    // Submit proof → milestone leaves Pending.
    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h0"),
        &ipfs(&t.env),
    );

    let payees = vec![
        &t.env,
        MilestonePayee {
            payee: t.supplier.clone(),
            percent: 100,
        },
    ];
    client.set_milestone_payees(&t.buyer, &ship_id, &0u32, &payees);
}

#[test]
fn test_two_way_payee_split_on_confirm() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let payee2 = Address::generate(&t.env);
    let ship_id = sid(&t.env, "payees_2way");

    // Single 100% milestone for simple accounting.
    let single_ms = vec![
        &t.env,
        Milestone {
            name: String::from_str(&t.env, "All"),
            payment_percent: 100,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ];

    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &single_ms,
        &default_options(&t.env),
    );

    // 70/30 split.
    let payees = vec![
        &t.env,
        MilestonePayee {
            payee: t.supplier.clone(),
            percent: 70,
        },
        MilestonePayee {
            payee: payee2.clone(),
            percent: 30,
        },
    ];
    client.set_milestone_payees(&t.buyer, &ship_id, &0u32, &payees);

    // Record balances before confirm.
    let token_client = soroban_sdk::token::Client::new(&t.env, &t.token_id);
    let before_supplier = token_client.balance(&t.supplier);
    let before_payee2 = token_client.balance(&payee2);

    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h0"),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);

    let after_supplier = token_client.balance(&t.supplier);
    let after_payee2 = token_client.balance(&payee2);

    let supplier_recv = after_supplier - before_supplier;
    let payee2_recv = after_payee2 - before_payee2;

    // Supplier should receive 70%, payee2 30% (no protocol fee in test).
    assert_eq!(supplier_recv, 700_000i128, "supplier should get 70%");
    assert_eq!(payee2_recv, 300_000i128, "payee2 should get 30%");
}

#[test]
fn test_three_way_payee_split_on_confirm() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let payee2 = Address::generate(&t.env);
    let payee3 = Address::generate(&t.env);
    let ship_id = sid(&t.env, "payees_3way");

    let single_ms = vec![
        &t.env,
        Milestone {
            name: String::from_str(&t.env, "All"),
            payment_percent: 100,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ];

    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_200_000i128,
        &single_ms,
        &default_options(&t.env),
    );

    // 50/30/20 split.
    let payees = vec![
        &t.env,
        MilestonePayee {
            payee: t.supplier.clone(),
            percent: 50,
        },
        MilestonePayee {
            payee: payee2.clone(),
            percent: 30,
        },
        MilestonePayee {
            payee: payee3.clone(),
            percent: 20,
        },
    ];
    client.set_milestone_payees(&t.buyer, &ship_id, &0u32, &payees);

    let token_client = soroban_sdk::token::Client::new(&t.env, &t.token_id);
    let before_s = token_client.balance(&t.supplier);
    let before_2 = token_client.balance(&payee2);
    let before_3 = token_client.balance(&payee3);

    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h0"),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);

    assert_eq!(
        token_client.balance(&t.supplier) - before_s,
        600_000i128,
        "50%"
    );
    assert_eq!(token_client.balance(&payee2) - before_2, 360_000i128, "30%");
    assert_eq!(token_client.balance(&payee3) - before_3, 240_000i128, "20%");
}

#[test]
fn test_payee_split_rounding_remainder_goes_to_last() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let payee2 = Address::generate(&t.env);
    let payee3 = Address::generate(&t.env);
    let ship_id = sid(&t.env, "payees_rounding");

    // 1_000_003 does not divide evenly by 100, so a remainder is left over
    // after the non-last shares are computed via integer division.
    let single_ms = vec![
        &t.env,
        Milestone {
            name: String::from_str(&t.env, "All"),
            payment_percent: 100,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        }
    ];

    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier, &t.logistics, &t.arbiter,
        &t.token_id, &1_000_003i128,
        &single_ms,
        &default_options(&t.env),
    );

    // 50/30/20 split. Expected via integer division:
    //   supplier (50) = 1_000_003 * 50 / 100 = 500_001
    //   payee2   (30) = 1_000_003 * 30 / 100 = 300_000
    //   payee3  (last, 20) = remainder = 1_000_003 - 800_001 = 200_002
    let payees = vec![
        &t.env,
        MilestonePayee { payee: t.supplier.clone(), percent: 50 },
        MilestonePayee { payee: payee2.clone(),    percent: 30 },
        MilestonePayee { payee: payee3.clone(),    percent: 20 },
    ];
    client.set_milestone_payees(&t.buyer, &ship_id, &0u32, &payees);

    let token_client = soroban_sdk::token::Client::new(&t.env, &t.token_id);
    let before_s = token_client.balance(&t.supplier);
    let before_2 = token_client.balance(&payee2);
    let before_3 = token_client.balance(&payee3);

    client.submit_proof(&t.supplier, &ship_id, &0u32, &sid(&t.env, "h0"), &ipfs(&t.env));
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);

    assert_eq!(token_client.balance(&t.supplier) - before_s, 500_001i128, "50% (floor)");
    assert_eq!(token_client.balance(&payee2) - before_2,     300_000i128, "30% (floor)");
    assert_eq!(token_client.balance(&payee3) - before_3,     200_002i128, "remainder to last payee");
}

#[test]
fn test_no_payees_configured_falls_back_to_supplier() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "payees_fallback");

    let single_ms = vec![
        &t.env,
        Milestone {
            name: String::from_str(&t.env, "All"),
            payment_percent: 100,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ];

    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &single_ms,
        &default_options(&t.env),
    );

    // No set_milestone_payees call → single supplier payout.
    let token_client = soroban_sdk::token::Client::new(&t.env, &t.token_id);
    let before = token_client.balance(&t.supplier);

    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h0"),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer, &ship_id, &0u32);

    assert_eq!(
        token_client.balance(&t.supplier) - before,
        1_000_000i128,
        "entire payment should go to supplier when no payees configured"
    );
}

// ═══════════════════════════════════════════════════════════════════
// FEATURE D – AUTO-BLACKLIST RULES
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_auto_blacklist_disabled_by_default() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let rule = client.get_auto_blacklist_rule();
    assert_eq!(rule.max_cancelled, 0, "max_cancelled should default to 0");
    assert_eq!(rule.max_disputed, 0, "max_disputed should default to 0");
}

#[test]
fn test_auto_blacklist_not_triggered_when_disabled() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let ship_id = sid(&t.env, "no_auto_bl");

    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    // Cancel: increments cancelled counter.
    client.cancel_shipment(&t.buyer, &ship_id);

    // Rule disabled (both 0) → supplier must not be blacklisted.
    assert!(
        !client.is_blacklisted(&t.supplier),
        "supplier should not be auto-blacklisted when rule is disabled"
    );
}

#[test]
fn test_auto_blacklist_triggered_by_cancelled_threshold() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // Auto-blacklist after 1 cancellation.
    client.set_auto_blacklist_rule(&t.buyer, &1u32, &0u32);

    let ship_id = sid(&t.env, "cancel_bl");
    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    client.cancel_shipment(&t.buyer, &ship_id);

    assert!(
        client.is_blacklisted(&t.supplier),
        "supplier should be auto-blacklisted after reaching cancelled threshold"
    );
}

#[test]
fn test_auto_blacklist_triggered_by_disputed_threshold() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // Auto-blacklist after 1 dispute.
    client.set_auto_blacklist_rule(&t.buyer, &0u32, &1u32);

    let ship_id = sid(&t.env, "dispute_bl");
    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    client.submit_proof(
        &t.supplier,
        &ship_id,
        &0u32,
        &sid(&t.env, "h0"),
        &ipfs(&t.env),
    );
    client.raise_dispute(&t.buyer, &ship_id, &0u32);

    assert!(
        client.is_blacklisted(&t.supplier),
        "supplier should be auto-blacklisted after reaching disputed threshold"
    );
}

#[test]
fn test_auto_blacklist_log_entry_distinguishable() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.set_auto_blacklist_rule(&t.buyer, &1u32, &0u32);

    let ship_id = sid(&t.env, "bl_log");
    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    client.cancel_shipment(&t.buyer, &ship_id);

    // The admin log should contain an entry with detail = "auto_blacklist_triggered".
    let log = client.get_admin_log();
    let auto_detail = Symbol::new(&t.env, "auto_blacklist_triggered");
    let found = (0..log.len()).any(|i| log.get(i as u32).unwrap().detail == auto_detail);
    assert!(
        found,
        "admin log should contain an auto_blacklist_triggered entry"
    );
}

#[test]
fn test_manual_remove_from_blacklist_works_after_auto_trigger() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.set_auto_blacklist_rule(&t.buyer, &1u32, &0u32);

    let ship_id = sid(&t.env, "bl_remove");
    client.create_shipment(
        &ship_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    client.cancel_shipment(&t.buyer, &ship_id);

    assert!(client.is_blacklisted(&t.supplier), "should be blacklisted");

    // Admin manually removes from blacklist.
    client.remove_from_blacklist(&t.buyer, &t.supplier);
    assert!(
        !client.is_blacklisted(&t.supplier),
        "should no longer be blacklisted after manual removal"
    );
}
