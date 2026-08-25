#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, String,
};
use crate::test_common::*;

// ============================================================
// QUERY & READ-ONLY TESTS: COMPLETION PERCENTAGE
// ============================================================

#[test]
fn test_get_completion_percentage_fresh_shipment() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-FRESH");
    let total_amount: i128 = 1_000_000_000;

    create_standard_shipment(
        &client,
        &t.env,
        &shipment_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        total_amount,
    );

    // Freshly created shipment with no milestones confirmed should return 0%
    assert_eq!(client.get_completion_percentage(&shipment_id), 0);
}

#[test]
fn test_get_completion_percentage_partial_one_milestone() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-PARTIAL-1");
    let total_amount: i128 = 1_000_000_000;

    create_standard_shipment(
        &client,
        &t.env,
        &shipment_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        total_amount,
    );

    // Confirm first milestone (25%)
    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "ipfs://d"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    // Should return 25%
    assert_eq!(client.get_completion_percentage(&shipment_id), 25);
}

#[test]
fn test_get_completion_percentage_partial_two_milestones() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-PARTIAL-2");
    let total_amount: i128 = 1_000_000_000;

    create_standard_shipment(
        &client,
        &t.env,
        &shipment_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        total_amount,
    );

    // Confirm first milestone (25%)
    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "ipfs://d"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    // Confirm second milestone (50% cumulative = 75% total)
    client.submit_proof(
        &t.logistics,
        &shipment_id,
        &1,
        &String::from_str(&t.env, "ipfs://t"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &1);

    // Should return 75%
    assert_eq!(client.get_completion_percentage(&shipment_id), 75);
}

#[test]
fn test_get_completion_percentage_full_completion() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-FULL");
    let total_amount: i128 = 1_000_000_000;

    create_standard_shipment(
        &client,
        &t.env,
        &shipment_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        total_amount,
    );

    // Confirm all milestones
    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "ipfs://d"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    client.submit_proof(
        &t.logistics,
        &shipment_id,
        &1,
        &String::from_str(&t.env, "ipfs://t"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &1);

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &2,
        &String::from_str(&t.env, "ipfs://v"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &2);

    // Should return 100%
    assert_eq!(client.get_completion_percentage(&shipment_id), 100);
}

#[test]
fn test_get_completion_percentage_zero_released() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-ZERO");
    let total_amount: i128 = 100;

    create_standard_shipment(
        &client,
        &t.env,
        &shipment_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        total_amount,
    );

    // Before any confirmation, released_amount is 0, should return 0%
    assert_eq!(client.get_completion_percentage(&shipment_id), 0);

    // Confirm first milestone (25 out of 100 = 25%)
    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "ipfs://d"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    // (25 * 100) / 100 = 25%
    assert_eq!(client.get_completion_percentage(&shipment_id), 25);
}

// ============================================================
// COMPLETION PERCENTAGE: DOCUMENTED WEIGHT DERIVATION
// ------------------------------------------------------------
// These tests pin the behaviour documented in
// `docs/completion-percentage.md` and in the rustdoc on
// `ChainSettleContract::get_completion_percentage`, i.e. that the
// percentage is the settled share of the milestone payment weights:
//
//   settled = released_amount + total_advanced_amount
//   pct     = clamp(settled * 100 / total_amount, 0, 100)
//
// with each accumulation being one milestone weight
// (`total_amount * payment_percent / 100`, or `* splits_bps / 10_000`).
// ============================================================

/// Submits proof as the supplier and confirms the milestone as the buyer.
fn settle_milestone(
    client: &ChainSettleContractClient,
    t: &TestSetup,
    shipment_id: &String,
    milestone_index: u32,
) {
    client.submit_proof(
        &t.supplier,
        shipment_id,
        &milestone_index,
        &String::from_str(&t.env, "ipfs://proof"),
        &Symbol::new(&t.env, "ipfs"),
    );
    client.confirm_milestone(&t.buyer, shipment_id, &milestone_index);
}

/// Basis-point weights (`milestone_splits`, #160) take precedence over
/// `payment_percent`, and the integer division floors the reading (§3.2 of the doc).
#[test]
fn test_get_completion_percentage_bps_splits_truncates_down() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-BPS");
    let total_amount: i128 = 1_000_000_000;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &ShipmentOptions {
            // 33.33 % / 33.33 % / 33.34 % — deliberately different from the
            // 25/50/25 `payment_percent` values, which must be ignored.
            milestone_splits: soroban_sdk::vec![&t.env, 3_333u32, 3_333u32, 3_334u32],
            ..default_options(&t.env)
        },
    );

    assert_eq!(client.get_completion_percentage(&shipment_id), 0);

    // 3_333 bps of 1_000_000_000 = 333_300_000 → 33.33 % → floored to 33.
    settle_milestone(&client, &t, &shipment_id, 0);
    assert_eq!(
        client.get_shipment(&shipment_id).released_amount,
        333_300_000
    );
    assert_eq!(client.get_completion_percentage(&shipment_id), 33);

    // Cumulative 666_600_000 → 66.66 % → floored to 66.
    settle_milestone(&client, &t, &shipment_id, 1);
    assert_eq!(client.get_completion_percentage(&shipment_id), 66);

    // Final 3_334 bps closes the escrow exactly → 100 %.
    settle_milestone(&client, &t, &shipment_id, 2);
    assert_eq!(
        client.get_shipment(&shipment_id).released_amount,
        total_amount
    );
    assert_eq!(client.get_completion_percentage(&shipment_id), 100);
    // Documented identity with the complementary query.
    assert_eq!(client.get_escrow_balance(&shipment_id), 0);
}

/// An approved advance is a *fraction* of one milestone weight: it counts through
/// `total_advanced_amount` while outstanding, then exactly once more as part of the
/// full milestone weight when the milestone is confirmed (§3.3 of the doc).
#[test]
fn test_get_completion_percentage_counts_approved_advance() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-ADVANCE");
    let total_amount: i128 = 1_000_000_000;

    create_standard_shipment(
        &client,
        &t.env,
        &shipment_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        total_amount,
    );

    // 20 % advance on milestone 0 (weight 25 %) = 50_000_000 = 5 % of the shipment.
    client.request_advance(&t.supplier, &shipment_id, &0, &20);
    client.approve_advance(&t.buyer, &shipment_id, &0);

    let after_advance = client.get_shipment(&shipment_id);
    assert_eq!(after_advance.released_amount, 0);
    assert_eq!(after_advance.total_advanced_amount, 50_000_000);
    assert_eq!(client.get_completion_percentage(&shipment_id), 5);

    // Confirming the milestone swaps the advance for the full 25 % weight — no double count.
    settle_milestone(&client, &t, &shipment_id, 0);

    let after_confirm = client.get_shipment(&shipment_id);
    assert_eq!(after_confirm.released_amount, 250_000_000);
    assert_eq!(after_confirm.total_advanced_amount, 0);
    assert_eq!(client.get_completion_percentage(&shipment_id), 25);
}

/// A confirmed-but-held milestone (`holdback_ledgers > 0`) is not counted until
/// `release_held_payment` actually moves the money (§3.4 of the doc).
#[test]
fn test_get_completion_percentage_excludes_held_payment() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-HELD");
    let total_amount: i128 = 1_000_000_000;
    let holdback: u32 = 100;

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &total_amount,
        &build_milestones(&t.env),
        &ShipmentOptions {
            holdback_ledgers: holdback,
            ..default_options(&t.env)
        },
    );

    settle_milestone(&client, &t, &shipment_id, 0);

    // Confirmed, but still inside the holdback window → nothing settled yet.
    assert_eq!(
        client.get_milestone(&shipment_id, &0).status,
        MilestoneStatus::ConfirmedHeld
    );
    assert_eq!(client.get_completion_percentage(&shipment_id), 0);

    t.env.ledger().set_sequence_number(holdback + 1);
    client.release_held_payment(&shipment_id, &0);

    assert_eq!(
        client.get_milestone(&shipment_id, &0).status,
        MilestoneStatus::Confirmed
    );
    assert_eq!(client.get_completion_percentage(&shipment_id), 25);
}

/// The reading tracks escrow *settlement*, not supplier earnings: the contested share
/// of a partial dispute counts even when it is refunded to the buyer (§3.5 of the doc).
#[test]
fn test_get_completion_percentage_counts_buyer_refund_settlement() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-REFUND");
    let total_amount: i128 = 1_000_000_000;

    create_standard_shipment(
        &client,
        &t.env,
        &shipment_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        total_amount,
    );

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &String::from_str(&t.env, "ipfs://proof"),
        &Symbol::new(&t.env, "ipfs"),
    );

    // 40 % of milestone 0 contested → uncontested 60 % of 250_000_000 = 150_000_000 (15 %).
    client.raise_partial_dispute(&t.buyer, &shipment_id, &0, &40);
    assert_eq!(client.get_completion_percentage(&shipment_id), 15);

    // Arbiter sides with the buyer: the contested 100_000_000 is refunded, yet it has
    // still left escrow, so the full 25 % weight now reads as settled.
    client.resolve_dispute(&t.arbiter, &shipment_id, &0, &false);

    assert_eq!(
        client.get_shipment(&shipment_id).released_amount,
        250_000_000
    );
    assert_eq!(client.get_completion_percentage(&shipment_id), 25);
}

/// The denominator is the live `total_amount`, so a top-up dilutes progress (§3.6).
#[test]
fn test_get_completion_percentage_diluted_by_top_up() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-TOPUP");
    let total_amount: i128 = 1_000_000_000;

    create_standard_shipment(
        &client,
        &t.env,
        &shipment_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        total_amount,
    );

    settle_milestone(&client, &t, &shipment_id, 0);
    assert_eq!(client.get_completion_percentage(&shipment_id), 25);

    // Doubling the escrow halves the settled share: 250_000_000 / 2_000_000_000 = 12.5 % → 12.
    client.top_up_escrow(&t.buyer, &shipment_id, &total_amount);
    assert_eq!(
        client.get_shipment(&shipment_id).total_amount,
        2_000_000_000
    );
    assert_eq!(client.get_completion_percentage(&shipment_id), 12);
}

/// Fees are payout-side deductions: the full milestone weight is still credited, so the
/// percentage is unaffected by the platform fee (§4 of the doc).
#[test]
fn test_get_completion_percentage_unaffected_by_platform_fee() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let token_client = token::Client::new(&t.env, &t.token_id);

    // `setup()` initialises the contract with `t.buyer` as admin.
    client.set_fee_config(&t.buyer, &500, &t.treasury);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-FEE");
    let total_amount: i128 = 1_000_000_000;

    create_standard_shipment(
        &client,
        &t.env,
        &shipment_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        total_amount,
    );

    settle_milestone(&client, &t, &shipment_id, 0);

    // 5 % of the 250_000_000 milestone weight went to the treasury...
    assert_eq!(token_client.balance(&t.treasury), 12_500_000);
    assert_eq!(token_client.balance(&t.supplier), 237_500_000);
    // ...but the weight itself is fully settled.
    assert_eq!(
        client.get_shipment(&shipment_id).released_amount,
        250_000_000
    );
    assert_eq!(client.get_completion_percentage(&shipment_id), 25);
}

/// Tiny escrows still map weights exactly: 25 % of 4 units = 1 unit = 25 %.
#[test]
fn test_get_completion_percentage_small_escrow_exact() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "SHIP-COMPL-SMALL");
    let total_amount: i128 = 4; // weights 25/50/25 → 1 / 2 / 1

    create_standard_shipment(
        &client,
        &t.env,
        &shipment_id,
        &t.buyer,
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        total_amount,
    );

    assert_eq!(client.get_completion_percentage(&shipment_id), 0);

    settle_milestone(&client, &t, &shipment_id, 0);
    assert_eq!(client.get_shipment(&shipment_id).released_amount, 1);
    assert_eq!(client.get_completion_percentage(&shipment_id), 25);
}
