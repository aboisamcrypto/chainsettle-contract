#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{default_options, single_buyer_vec, TestSetup};
use soroban_sdk::{vec, Env, String};

// ============================================================
// TEST SETUP
// ============================================================

fn setup() -> TestSetup {
    let t = crate::test_common::setup();
    // This file mints a much larger buyer balance than the shared fixture
    // to cover its large-amount boundary cases.
    let token_client = soroban_sdk::token::StellarAssetClient::new(&t.env, &t.token_id);
    token_client.mint(&t.buyer, &1_000_000_000_000);
    t
}

fn build_valid_milestone(env: &Env) -> soroban_sdk::Vec<Milestone> {
    vec![
        env,
        Milestone {
            name: String::from_str(env, "Milestone 1"),
            payment_percent: 100,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ]
}

fn build_zero_percent_milestone(env: &Env) -> soroban_sdk::Vec<Milestone> {
    vec![
        env,
        Milestone {
            name: String::from_str(env, "Zero Percent Milestone"),
            payment_percent: 0,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ]
}

fn build_low_percent_milestone(env: &Env) -> soroban_sdk::Vec<Milestone> {
    vec![
        env,
        Milestone {
            name: String::from_str(env, "Low Percent Milestone"),
            payment_percent: 1,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ]
}

fn build_multi_milestone_with_zero(env: &Env) -> soroban_sdk::Vec<Milestone> {
    vec![
        env,
        Milestone {
            name: String::from_str(env, "Milestone 1"),
            payment_percent: 50,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
        Milestone {
            name: String::from_str(env, "Milestone 2 (Zero Percent)"),
            payment_percent: 0,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
        Milestone {
            name: String::from_str(env, "Milestone 3"),
            payment_percent: 50,
            proof_hash: String::from_str(env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ]
}

// ============================================================
// TEST 1: Zero total_amount Rejected
// ============================================================

#[test]
#[should_panic(expected = "amount must be greater than zero")]
fn test_create_shipment_zero_total_amount_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-ZERO-AMT");
    let total_amount: i128 = 0;

    // Attempt to create shipment with zero total_amount
    // Should panic: "amount must be greater than zero"
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_valid_milestone(&t.env),
        &default_options(&t.env),
    );
}

// ============================================================
// TEST 2: Negative total_amount (-1) Rejected
// ============================================================

#[test]
#[should_panic(expected = "amount must be greater than zero")]
fn test_create_shipment_negative_total_amount_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-NEG-AMT");
    let total_amount: i128 = -1;

    // Attempt to create shipment with negative total_amount
    // Should panic: "amount must be greater than zero"
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_valid_milestone(&t.env),
        &default_options(&t.env),
    );
}

// ============================================================
// TEST 3: Large Negative total_amount Rejected
// ============================================================

#[test]
#[should_panic(expected = "amount must be greater than zero")]
fn test_create_shipment_large_negative_total_amount_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-LARGE-NEG");
    let total_amount: i128 = -1_000_000_000;

    // Attempt to create shipment with large negative total_amount
    // Should panic: "amount must be greater than zero"
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_valid_milestone(&t.env),
        &default_options(&t.env),
    );
}

// ============================================================
// TEST 4: Single Milestone with payment_percent = 0 Rejected
// ============================================================

#[test]
#[should_panic(expected = "InvalidPercentages")]
fn test_create_shipment_single_milestone_zero_percent_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-ZERO-PCT");
    let total_amount: i128 = 1_000_000_000;

    // Attempt to create shipment with single milestone at 0%
    // Should panic: "InvalidPercentages"
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_zero_percent_milestone(&t.env),
        &default_options(&t.env),
    );
}

// ============================================================
// TEST 5: Multiple Milestones with One at 0% Rejected
// ============================================================

#[test]
#[should_panic(expected = "InvalidPercentages")]
fn test_create_shipment_multi_milestone_with_zero_percent_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-MULTI-ZERO");
    let total_amount: i128 = 1_000_000_000;

    // Attempt to create shipment with 3 milestones, middle one at 0%
    // Should panic: "InvalidPercentages"
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_multi_milestone_with_zero(&t.env),
        &default_options(&t.env),
    );
}

// ============================================================
// TEST 6: Minimum Valid Amount (1) Accepted
// ============================================================

#[test]
fn test_create_shipment_minimum_valid_amount_accepted() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-MIN-VALID");
    let total_amount: i128 = 1; // Minimum valid amount

    // Create shipment with minimum valid amount
    // Should succeed (no panic)
    let result = client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_valid_milestone(&t.env),
        &default_options(&t.env),
    );

    // Verify shipment was created successfully
    assert_eq!(result, shipment_id);
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.total_amount, 1);
    assert_eq!(shipment.status, ShipmentStatus::Active);
}

// ============================================================
// TEST 7: Small Valid Amount (100) Accepted
// ============================================================

#[test]
fn test_create_shipment_small_valid_amount_accepted() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-SMALL-VALID");
    let total_amount: i128 = 100;

    // Create shipment with small valid amount
    // Should succeed (no panic)
    let result = client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_valid_milestone(&t.env),
        &default_options(&t.env),
    );

    // Verify shipment was created successfully
    assert_eq!(result, shipment_id);
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.total_amount, 100);
    assert_eq!(shipment.status, ShipmentStatus::Active);
}

// ============================================================
// TEST 8: Milestone with Min Percent (5%) Accepted
// ============================================================

#[test]
fn test_create_shipment_milestone_min_percent_accepted() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-MIN-PCT");
    let total_amount: i128 = 1_000_000_000;

    // Create milestone with minimum valid percentage (default min_pct = 5%)
    let milestones = vec![
        &t.env,
        Milestone {
            name: String::from_str(&t.env, "Milestone 1"),
            payment_percent: 5, // Minimum valid percentage
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
        Milestone {
            name: String::from_str(&t.env, "Milestone 2"),
            payment_percent: 95,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ];

    // Should succeed with minimum valid percentages
    let result = client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &milestones,
        &default_options(&t.env),
    );

    assert_eq!(result, shipment_id);
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.milestones.get(0).unwrap().payment_percent, 5);
    assert_eq!(shipment.milestones.get(1).unwrap().payment_percent, 95);
}

// ============================================================
// TEST 9: Below Minimum Percentage (4%) Rejected
// ============================================================

#[test]
#[should_panic(expected = "InvalidPercentages")]
fn test_create_shipment_below_min_percent_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-BELOW-MIN-PCT");
    let total_amount: i128 = 1_000_000_000;

    // Create milestone with percentage below minimum (< 5%)
    let milestones = vec![
        &t.env,
        Milestone {
            name: String::from_str(&t.env, "Milestone 1"),
            payment_percent: 4, // Below minimum (< 5%)
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
        Milestone {
            name: String::from_str(&t.env, "Milestone 2"),
            payment_percent: 96,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ];

    // Should panic: "InvalidPercentages"
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &milestones,
        &default_options(&t.env),
    );
}

// ============================================================
// TEST 10: Large Valid Amount Accepted
// ============================================================

#[test]
fn test_create_shipment_large_valid_amount_accepted() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-LARGE-VALID");
    let total_amount: i128 = 999_999_999_999; // Large but valid amount

    // Create shipment with large valid amount
    let result = client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_valid_milestone(&t.env),
        &default_options(&t.env),
    );

    assert_eq!(result, shipment_id);
    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.total_amount, 999_999_999_999);
}

// ============================================================
// TEST 11: All Zero and Negative Amount Cases Summary
// ============================================================

#[test]
fn test_boundary_validation_coverage_summary() {
    // This test documents the comprehensive coverage of boundary validation:
    
    // Amount Validation:
    // ✓ Test 1: Zero amount (0) rejected
    // ✓ Test 2: Negative amount (-1) rejected
    // ✓ Test 3: Large negative amount rejected
    // ✓ Test 6: Minimum valid amount (1) accepted
    // ✓ Test 7: Small valid amount (100) accepted
    // ✓ Test 10: Large valid amount accepted
    
    // Milestone Percentage Validation:
    // ✓ Test 4: Single milestone with 0% rejected
    // ✓ Test 5: Multiple milestones with one at 0% rejected
    // ✓ Test 8: Milestone at minimum valid % (5%) accepted
    // ✓ Test 9: Milestone below minimum % (4%) rejected
    
    // Error Messages Verified:
    // ✓ "amount must be greater than zero" for invalid amounts
    // ✓ "InvalidPercentages" for invalid milestone percentages
    
    // Boundary Test Matrix:
    // Zero/Negative: ✓ Covered
    // Minimum Valid: ✓ Covered
    // Valid Range: ✓ Covered
    // Large Values: ✓ Covered
    
    assert_eq!(1, 1); // Trivial assertion; this test documents coverage
}

// ============================================================
// TEST 12: Single Milestone Below Minimum Percentage Rejected
// ============================================================

#[test]
#[should_panic(expected = "InvalidPercentages")]
fn test_create_shipment_single_low_percent_milestone_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-SINGLE-LOW-PCT");
    let total_amount: i128 = 1_000_000_000;

    // Single milestone at 1% is below the default min_pct (5%).
    // Should panic: "InvalidPercentages"
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_low_percent_milestone(&t.env),
        &default_options(&t.env),
    );
}

// ============================================================
// TEST 13: Milestone Percentages Summing Above 100 Rejected
// ============================================================

#[test]
#[should_panic(expected = "milestone percentages must sum to 100")]
fn test_create_shipment_percentages_sum_above_100_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-SUM-OVER-100");
    let total_amount: i128 = 1_000_000_000;

    // Two milestones, each individually above min_pct, but summing to 110%.
    let milestones = vec![
        &t.env,
        Milestone {
            name: String::from_str(&t.env, "Milestone 1"),
            payment_percent: 60,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
        Milestone {
            name: String::from_str(&t.env, "Milestone 2"),
            payment_percent: 50,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ];

    // Should panic: "milestone percentages must sum to 100"
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &milestones,
        &default_options(&t.env),
    );
}

// ============================================================
// TEST 14: Milestone Percentages Summing Below 100 Rejected
// ============================================================

#[test]
#[should_panic(expected = "milestone percentages must sum to 100")]
fn test_create_shipment_percentages_sum_below_100_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-SUM-UNDER-100");
    let total_amount: i128 = 1_000_000_000;

    // Two milestones, each individually above min_pct, but summing to 90%.
    let milestones = vec![
        &t.env,
        Milestone {
            name: String::from_str(&t.env, "Milestone 1"),
            payment_percent: 40,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
        Milestone {
            name: String::from_str(&t.env, "Milestone 2"),
            payment_percent: 50,
            proof_hash: String::from_str(&t.env, ""),
            status: MilestoneStatus::Pending,
            release_after_ledger: 0,
            proof_submitted_ledger: None,
            dispute_opened_ledger: None,
            deadline_ledger: 0,
            penalty_bps_per_ledger: 0,
        },
    ];

    // Should panic: "milestone percentages must sum to 100"
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &milestones,
        &default_options(&t.env),
    );
}
