// #388 — VIP partner fee waiver via governance vote.
//
// Verifies propose_fee_waiver / approve_fee_waiver route through the same
// MultiAdminConfig multisig machinery used elsewhere (#166, #402), gated by
// the routine MultiAdminConfig.threshold, and that a granted waiver actually
// reduces the platform fee charged on milestone confirmation.

#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Address as _, vec, String};

#[test]
#[should_panic(expected = "multisig admin not configured")]
fn test_propose_fee_waiver_requires_multisig_config() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.propose_fee_waiver(&t.buyer, &t.supplier, &5000u32, &0u64);
}

#[test]
fn test_single_threshold_multisig_grants_waiver_immediately() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admins = vec![&t.env, t.buyer.clone(), Address::generate(&t.env)];
    client.initialize_multisig_admin(&t.buyer, &admins, &1u32);

    assert_eq!(client.get_fee_waiver(&t.supplier), None);
    let proposal_id = client.propose_fee_waiver(&t.buyer, &t.supplier, &5000u32, &0u64);
    assert_eq!(client.get_fee_waiver_proposal(&proposal_id), None); // already executed
    assert_eq!(client.get_fee_waiver(&t.supplier), Some((5000u32, 0u64)));
}

#[test]
fn test_waiver_requires_threshold_approvals() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admin2 = Address::generate(&t.env);
    let admin3 = Address::generate(&t.env);
    let admins = vec![&t.env, t.buyer.clone(), admin2.clone(), admin3.clone()];
    client.initialize_multisig_admin(&t.buyer, &admins, &2u32);

    let proposal_id = client.propose_fee_waiver(&t.buyer, &t.supplier, &2500u32, &0u64);
    assert_eq!(client.get_fee_waiver(&t.supplier), None);

    client.approve_fee_waiver(&admin2, &proposal_id);
    assert_eq!(client.get_fee_waiver(&t.supplier), Some((2500u32, 0u64)));
    assert_eq!(client.get_fee_waiver_proposal(&proposal_id), None);
}

#[test]
#[should_panic(expected = "already approved by this admin")]
fn test_admin_cannot_approve_twice() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admin2 = Address::generate(&t.env);
    let admin3 = Address::generate(&t.env);
    let admins = vec![&t.env, t.buyer.clone(), admin2.clone(), admin3.clone()];
    client.initialize_multisig_admin(&t.buyer, &admins, &2u32);

    let proposal_id = client.propose_fee_waiver(&t.buyer, &t.supplier, &2500u32, &0u64);
    client.approve_fee_waiver(&t.buyer, &proposal_id);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_non_admin_cannot_propose_waiver() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admins = vec![&t.env, t.buyer.clone()];
    client.initialize_multisig_admin(&t.buyer, &admins, &1u32);

    client.propose_fee_waiver(&t.supplier, &t.supplier, &5000u32, &0u64);
}

#[test]
#[should_panic(expected = "waiver_bps cannot exceed 10000")]
fn test_waiver_bps_capped_at_10000() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admins = vec![&t.env, t.buyer.clone()];
    client.initialize_multisig_admin(&t.buyer, &admins, &1u32);

    client.propose_fee_waiver(&t.buyer, &t.supplier, &10_001u32, &0u64);
}

#[test]
fn test_expired_waiver_no_longer_applies() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admins = vec![&t.env, t.buyer.clone()];
    client.initialize_multisig_admin(&t.buyer, &admins, &1u32);

    // Expiry in the past relative to the test env's ledger timestamp.
    client.propose_fee_waiver(&t.buyer, &t.supplier, &5000u32, &1u64);
    assert_eq!(client.get_fee_waiver(&t.supplier), None);
}

#[test]
fn test_granted_waiver_reduces_platform_fee_on_confirmation() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.set_fee_config(&t.buyer, &1000u32, &t.treasury); // 10% base fee

    let admins = vec![&t.env, t.buyer.clone()];
    client.initialize_multisig_admin(&t.buyer, &admins, &1u32);
    // Full waiver (100%) for the buyer paying the fee.
    client.propose_fee_waiver(&t.buyer, &t.buyer, &10_000u32, &0u64);

    let shipment_id = String::from_str(&t.env, "WAIVER-001");
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );

    let preview = client.preview_milestone_payout(&shipment_id, &0u32);
    // 25% of 1_000_000 = 250_000 gross; fully waived fee => net == gross.
    assert_eq!(preview.gross_amount, 250_000);
    assert_eq!(preview.platform_fee, 0);
    assert_eq!(preview.supplier_net_amount, 250_000);
}
