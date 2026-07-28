#![cfg(test)]
// #161 — Multi-token support: XLM and EURC alongside USDC.
//
// `create_shipment` already takes a `token: Address` parameter, stores it on the
// Shipment, and every payout path (confirm_milestone, cancel_shipment, dispute
// resolution, ...) transfers through `token::Client::new(&env, &shipment.token)` —
// so any Stellar Asset Contract works, not just USDC. On a real network XLM and
// EURC are just different SAC addresses (native XLM's wrapper contract, and
// Circle's EURC issuer contract, respectively) — from the contract's point of
// view they are indistinguishable from USDC. These tests simulate that with three
// independently registered SAC tokens standing in for USDC / XLM / EURC, and
// exercise the admin-managed whitelist described in the issue.

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Address as _, token, String, Symbol};

fn sid(env: &Env, id: &str) -> String {
    String::from_str(env, id)
}

/// Registers a fresh SAC token and mints `amount` to `to`. Stands in for USDC,
/// XLM's native asset wrapper, or EURC — the contract treats all SACs identically.
fn register_and_fund(env: &Env, to: &Address, amount: i128) -> Address {
    let token_admin = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    token::StellarAssetClient::new(env, &token_id).mint(to, &amount);
    token_id
}

#[test]
fn test_shipment_lifecycle_works_with_usdc_xlm_and_eurc() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // t.token_id from the shared test harness stands in for USDC.
    let usdc = t.token_id.clone();
    let xlm = register_and_fund(&t.env, &t.buyer, 10_000_000_000);
    let eurc = register_and_fund(&t.env, &t.buyer, 10_000_000_000);

    for (label, token_id) in [("usdc", &usdc), ("xlm", &xlm), ("eurc", &eurc)] {
        let shipment_id = sid(&t.env, &std::format!("multi-token-{label}"));
        let token_client = token::Client::new(&t.env, token_id);
        let supplier_balance_before = token_client.balance(&t.supplier);

        client.create_shipment(
            &shipment_id,
            &single_buyer_vec(&t.env, &t.buyer),
            &t.supplier,
            &t.logistics,
            &t.arbiter,
            token_id,
            &1_000_000_000i128,
            &build_milestones(&t.env),
            &default_options(&t.env),
        );

        let shipment = client.get_shipment(&shipment_id);
        assert_eq!(&shipment.token, token_id, "shipment must store the token it was created with");

        client.submit_proof(
            &t.supplier,
            &shipment_id,
            &0,
            &String::from_str(&t.env, "proof0"),
            &Symbol::new(&t.env, "ipfs"),
        );
        client.confirm_milestone(&t.buyer, &shipment_id, &0);

        // Milestone 0 is 25% of 1B — payout must land in the *same* token the
        // shipment was created with, not some other escrow token.
        let supplier_balance_after = token_client.balance(&t.supplier);
        assert_eq!(
            supplier_balance_after - supplier_balance_before,
            250_000_000,
            "milestone payout for {label} must be denominated in the shipment's own token"
        );
    }
}

#[test]
fn test_whitelist_allows_approved_token() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let usdc = t.token_id.clone();

    // Admin approves only USDC.
    client.add_allowed_token(&usdc);
    assert_eq!(client.get_allowed_tokens(), soroban_sdk::vec![&t.env, usdc.clone()]);

    // USDC shipment still works.
    client.create_shipment(
        &sid(&t.env, "whitelist-usdc-ok"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &usdc,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

#[test]
#[should_panic(expected = "token is not in the approved whitelist")]
fn test_whitelist_rejects_non_approved_token() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let usdc = t.token_id.clone();
    let eurc = register_and_fund(&t.env, &t.buyer, 10_000_000_000);

    // Admin approves only USDC — EURC is not on the whitelist.
    client.add_allowed_token(&usdc);

    client.create_shipment(
        &sid(&t.env, "whitelist-eurc-rejected"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &eurc,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

#[test]
fn test_admin_can_add_approved_token() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let eurc = register_and_fund(&t.env, &t.buyer, 10_000_000_000);

    client.add_allowed_token(&eurc);
    assert_eq!(client.get_allowed_tokens().len(), 1);

    // Now approved — shipment creation succeeds.
    client.create_shipment(
        &sid(&t.env, "eurc-after-approval"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &eurc,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

#[test]
#[should_panic(expected = "token is not in the approved whitelist")]
fn test_admin_can_remove_approved_token() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let usdc = t.token_id.clone();
    let eurc = register_and_fund(&t.env, &t.buyer, 10_000_000_000);

    // Both USDC and EURC start out approved (list stays non-empty after removal).
    client.add_allowed_token(&usdc);
    client.add_allowed_token(&eurc);
    client.remove_allowed_token(&eurc);
    assert_eq!(client.get_allowed_tokens(), soroban_sdk::vec![&t.env, usdc]);

    // Removed — subsequent creation with EURC must be rejected again, even
    // though the whitelist itself is still non-empty (USDC remains approved).
    client.create_shipment(
        &sid(&t.env, "eurc-after-removal"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &eurc,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

#[test]
fn test_empty_whitelist_is_open_mode_for_any_token() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // No admin whitelist configured at all — any SAC token is accepted.
    assert_eq!(client.get_allowed_tokens().len(), 0);

    let xlm = register_and_fund(&t.env, &t.buyer, 10_000_000_000);
    client.create_shipment(
        &sid(&t.env, "open-mode-xlm"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &xlm,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

#[test]
fn test_cancellation_refunds_in_the_shipments_own_token() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let xlm = register_and_fund(&t.env, &t.buyer, 10_000_000_000);
    let token_client = token::Client::new(&t.env, &xlm);
    let buyer_balance_before = token_client.balance(&t.buyer);

    let shipment_id = sid(&t.env, "cancel-in-xlm");
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &xlm,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    client.cancel_shipment(&t.buyer, &shipment_id);

    assert_eq!(
        token_client.balance(&t.buyer),
        buyer_balance_before,
        "buyer must be refunded the full amount, in XLM, on cancellation"
    );
}
