#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Ledger as _, token, vec, String, Symbol};

fn sid(env: &Env, id: &str) -> String {
    String::from_str(env, id)
}

fn confirm_all_milestones(
    client: &ChainSettleContractClient,
    t: &crate::test_common::TestSetup,
    shipment_id: &String,
) {
    for i in 0..3 {
        client.submit_proof(
            &t.supplier,
            shipment_id,
            &i,
            &String::from_str(&t.env, "proof_hash"),
            &Symbol::new(&t.env, "ipfs"),
        );
        client.confirm_milestone(&t.buyer, shipment_id, &i);
    }
}

// ============================================================
// LOCKING COLLATERAL AT CREATION
// ============================================================

#[test]
fn test_collateral_locked_at_creation() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    mint_client.mint(&t.supplier, &100_000_000);
    let supplier_balance_before = token_client.balance(&t.supplier);

    let shipment_id = sid(&t.env, "collat-create-1");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 50_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    let supplier_balance_after = token_client.balance(&t.supplier);
    assert_eq!(
        supplier_balance_before - supplier_balance_after,
        50_000_000,
        "Supplier balance should decrease by exactly the collateral amount"
    );
}

#[test]
fn test_collateral_increases_contract_balance() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    mint_client.mint(&t.supplier, &100_000_000);
    let contract_balance_before = token_client.balance(&t.contract_id);

    let shipment_id = sid(&t.env, "collat-create-2");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 50_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    let contract_balance_after = token_client.balance(&t.contract_id);
    // Contract should hold total_amount (escrow) + supplier_collateral.
    assert_eq!(
        contract_balance_after - contract_balance_before,
        1_000_000_000 + 50_000_000,
        "Contract should custody both the escrow amount and the collateral"
    );
}

#[test]
fn test_zero_collateral_no_transfer_from_supplier() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let supplier_balance_before = token_client.balance(&t.supplier);

    let shipment_id = sid(&t.env, "collat-zero");
    let opts = default_options(&t.env); // supplier_collateral defaults to 0

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    let supplier_balance_after = token_client.balance(&t.supplier);
    assert_eq!(
        supplier_balance_before, supplier_balance_after,
        "No collateral should be pulled from the supplier when supplier_collateral is 0"
    );
}

#[test]
fn test_collateral_minimum_unit_locked() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    mint_client.mint(&t.supplier, &10);
    let supplier_balance_before = token_client.balance(&t.supplier);

    let shipment_id = sid(&t.env, "collat-min-unit");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 1;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    assert_eq!(
        supplier_balance_before - token_client.balance(&t.supplier),
        1,
        "The smallest nonzero collateral amount should still be locked"
    );
}

#[test]
fn test_collateral_larger_than_shipment_total() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);

    // Collateral bigger than the shipment's total escrow amount.
    mint_client.mint(&t.supplier, &5_000_000_000);

    let shipment_id = sid(&t.env, "collat-large");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 2_000_000_000; // larger than total_amount below

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Active);
}

#[test]
#[should_panic]
fn test_collateral_creation_panics_on_insufficient_supplier_balance() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    // Supplier has zero balance and no mint; requiring collateral must fail the transfer.

    let shipment_id = sid(&t.env, "collat-insufficient");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 50_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );
}

// ============================================================
// RETURN OF COLLATERAL ON COMPLETION
// ============================================================

#[test]
fn test_collateral_returned_on_full_completion() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    mint_client.mint(&t.supplier, &100_000_000);
    let supplier_balance_before = token_client.balance(&t.supplier);

    let shipment_id = sid(&t.env, "collat-complete");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 50_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    confirm_all_milestones(&client, &t, &shipment_id);

    let supplier_balance_after = token_client.balance(&t.supplier);
    // Supplier should net: -collateral (locked) + collateral (returned) + full milestone payouts.
    assert_eq!(
        supplier_balance_after - supplier_balance_before,
        1_000_000_000,
        "Supplier should receive full milestone payments plus the returned collateral"
    );
}

#[test]
fn test_collateral_not_returned_after_one_of_three_milestones() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    mint_client.mint(&t.supplier, &100_000_000);
    let supplier_balance_before = token_client.balance(&t.supplier);

    let shipment_id = sid(&t.env, "collat-partial-1");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 50_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "proof_hash_0"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    let supplier_balance_after = token_client.balance(&t.supplier);
    // Only the first milestone (25% of 1B = 250M) has been paid; collateral (50M) is still locked.
    assert_eq!(
        supplier_balance_after - supplier_balance_before,
        250_000_000 - 50_000_000,
        "Collateral must remain locked until all milestones are confirmed"
    );
}

#[test]
fn test_collateral_not_returned_after_two_of_three_milestones() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    mint_client.mint(&t.supplier, &100_000_000);
    let supplier_balance_before = token_client.balance(&t.supplier);

    let shipment_id = sid(&t.env, "collat-partial-2");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 50_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    for i in 0..2 {
        client.submit_proof(
            &t.supplier,
            &shipment_id,
            &i,
            &String::from_str(&t.env, "proof_hash"),
            &Symbol::new(&t.env, "ipfs"),
        );
        client.confirm_milestone(&t.buyer, &shipment_id, &i);
    }

    let supplier_balance_after = token_client.balance(&t.supplier);
    // First two milestones = 25% + 50% = 75% of 1B = 750M paid; collateral (50M) still locked.
    assert_eq!(
        supplier_balance_after - supplier_balance_before,
        750_000_000 - 50_000_000,
        "Collateral must remain locked until the final milestone is confirmed"
    );
}

#[test]
fn test_collateral_return_independent_of_logistics_fee() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    mint_client.mint(&t.supplier, &100_000_000);
    let supplier_balance_before = token_client.balance(&t.supplier);

    let shipment_id = sid(&t.env, "collat-logistics-fee");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 50_000_000;
    opts.logistics_fee_bps = 500; // 5% logistics fee on each milestone payout

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    confirm_all_milestones(&client, &t, &shipment_id);

    let logistics_balance = token_client.balance(&t.logistics);
    assert_eq!(logistics_balance, 50_000_000, "Logistics fee = 5% of 1B");

    let supplier_balance_after = token_client.balance(&t.supplier);
    // Supplier nets: milestone payouts (1B - 50M logistics fee) + returned collateral (net zero).
    assert_eq!(
        supplier_balance_after - supplier_balance_before,
        950_000_000,
        "Collateral should be returned in full regardless of the logistics fee split"
    );
}

// ============================================================
// FORFEITURE OF COLLATERAL ON BUYER CANCELLATION
// ============================================================

#[test]
fn test_collateral_forfeited_on_buyer_cancel_before_any_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = sid(&t.env, "collat-cancel-1");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 50_000_000;

    mint_client.mint(&t.supplier, &100_000_000);

    let buyer_balance_before = token_client.balance(&t.buyer);

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    client.cancel_shipment(&t.buyer, &shipment_id);

    let buyer_balance_after = token_client.balance(&t.buyer);
    // Buyer paid 1B into escrow, gets the full 1B refund back plus the 50M forfeited collateral.
    assert_eq!(
        buyer_balance_after - buyer_balance_before,
        50_000_000,
        "Buyer should net the forfeited collateral on top of the full refund"
    );
}

#[test]
fn test_collateral_forfeited_on_buyer_cancel_after_partial_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = sid(&t.env, "collat-cancel-2");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 50_000_000;

    mint_client.mint(&t.supplier, &100_000_000);

    let buyer_balance_before = token_client.balance(&t.buyer);

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "proof_hash_0"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    client.cancel_shipment(&t.buyer, &shipment_id);

    let buyer_balance_after = token_client.balance(&t.buyer);
    // Buyer paid 1B, milestone 0 (250M) was released to supplier, remaining 750M is refunded,
    // plus the forfeited 50M collateral.
    assert_eq!(
        buyer_balance_after - buyer_balance_before,
        -250_000_000 + 50_000_000,
        "Buyer should receive the unreleased refund plus forfeited collateral"
    );
}

#[test]
fn test_collateral_forfeiture_combined_with_buyer_cancel_fee() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = sid(&t.env, "collat-cancel-fee");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 50_000_000;
    opts.buyer_cancel_fee_bps = 1000; // 10% cancel fee to supplier

    mint_client.mint(&t.supplier, &100_000_000);

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    let supplier_balance_before_cancel = token_client.balance(&t.supplier);
    let buyer_balance_before_cancel = token_client.balance(&t.buyer);

    client.cancel_shipment(&t.buyer, &shipment_id);

    // Cancel fee = 10% of the unreleased 1B = 100M, paid to supplier (collateral is not touched here).
    let supplier_balance_after_cancel = token_client.balance(&t.supplier);
    assert_eq!(
        supplier_balance_after_cancel - supplier_balance_before_cancel,
        100_000_000,
        "Supplier should only receive the cancel fee, not the collateral"
    );

    // Buyer gets the remaining 900M refund plus the forfeited 50M collateral.
    let buyer_balance_after_cancel = token_client.balance(&t.buyer);
    assert_eq!(
        buyer_balance_after_cancel - buyer_balance_before_cancel,
        900_000_000 + 50_000_000,
        "Buyer should receive the fee-adjusted refund plus forfeited collateral"
    );
}

// ============================================================
// FORFEITURE OF COLLATERAL ON DEADLINE-BASED REFUND (#164)
// ============================================================

#[test]
fn test_collateral_forfeited_on_claim_deadline_refund() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    let shipment_id = sid(&t.env, "collat-deadline");
    let mut opts = default_options(&t.env);
    opts.supplier_collateral = 50_000_000;
    opts.deadlines = vec![&t.env, 1_000u64, 1_000u64, 1_000u64];

    mint_client.mint(&t.supplier, &100_000_000);

    let buyer_balance_before = token_client.balance(&t.buyer);

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    // Advance the ledger timestamp past the deadline without confirming any milestone.
    t.env.ledger().with_mut(|li| li.timestamp = 2_000);

    client.claim_deadline_refund(&t.buyer, &shipment_id, &0);

    let buyer_balance_after = token_client.balance(&t.buyer);
    // buyer_balance_before was captured pre-creation, so the 1B paid in and the 1B refunded
    // net out to zero; only the forfeited 50M collateral shows up as a net gain.
    assert_eq!(
        buyer_balance_after - buyer_balance_before,
        50_000_000,
        "Buyer should recover the forfeited collateral once the deadline lapses"
    );

    let shipment = client.get_shipment(&shipment_id);
    assert_eq!(shipment.status, ShipmentStatus::Expired);
}

// ============================================================
// STATE ISOLATION ACROSS SHIPMENTS
// ============================================================

#[test]
fn test_multiple_shipments_have_independent_collateral() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let mint_client = token::StellarAssetClient::new(&t.env, &t.token_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    mint_client.mint(&t.supplier, &200_000_000);

    let shipment_a = sid(&t.env, "collat-multi-a");
    let mut opts_a = default_options(&t.env);
    opts_a.supplier_collateral = 30_000_000;

    let shipment_b = sid(&t.env, "collat-multi-b");
    let mut opts_b = default_options(&t.env);
    opts_b.supplier_collateral = 70_000_000;

    client.create_shipment(
        &shipment_a,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts_a,
    );
    client.create_shipment(
        &shipment_b,
        &single_buyer_vec(&t.env, &t.buyer2),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &opts_b,
    );

    let supplier_balance_after_creation = token_client.balance(&t.supplier);
    assert_eq!(
        200_000_000 - supplier_balance_after_creation,
        30_000_000 + 70_000_000,
        "Both shipments' collateral should be locked independently from the supplier"
    );

    // Completing shipment A pays out its milestones and returns only its own 30M collateral
    // (shipment B's 70M stays locked).
    confirm_all_milestones(&client, &t, &shipment_a);
    let supplier_balance_after_a = token_client.balance(&t.supplier);
    assert_eq!(
        supplier_balance_after_a - supplier_balance_after_creation,
        1_000_000_000 + 30_000_000,
        "Completing shipment A should return only shipment A's collateral"
    );

    // Cancelling shipment B refunds its full unreleased escrow (1B, untouched by shipment A)
    // and forfeits its own 70M collateral to buyer2.
    let buyer2_balance_before = token_client.balance(&t.buyer2);
    client.cancel_shipment(&t.buyer2, &shipment_b);
    let buyer2_balance_after = token_client.balance(&t.buyer2);
    assert_eq!(
        buyer2_balance_after - buyer2_balance_before,
        1_000_000_000 + 70_000_000,
        "Cancelling shipment B should forfeit only shipment B's collateral"
    );
}
