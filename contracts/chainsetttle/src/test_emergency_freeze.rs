// #402 — Emergency global freeze requiring supermajority multisig approval.
//
// Verifies propose_emergency_freeze / approve_emergency_freeze (and the
// unfreeze mirror) against the same MultiAdminConfig used by the upgrade
// multisig (#166), but gated by a stricter, independently configurable
// supermajority rather than the routine MultiAdminConfig.threshold — and
// that it falls back to single-admin pause()-style semantics when multisig
// admin governance isn't configured at all.

#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Address as _, String};

fn sid(env: &Env, id: &str) -> String {
    String::from_str(env, id)
}

#[test]
fn test_single_admin_fallback_activates_and_lifts_freeze_immediately() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // No multisig admin governance configured — falls back to pause()-style
    // single-admin semantics.
    assert!(!client.is_emergency_frozen());
    let proposal_id = client.propose_emergency_freeze(&t.buyer);
    assert_eq!(proposal_id, 0);
    assert!(client.is_emergency_frozen());

    client.propose_emergency_unfreeze(&t.buyer);
    assert!(!client.is_emergency_frozen());
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_single_admin_fallback_rejects_non_admin() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.propose_emergency_freeze(&t.supplier);
}

#[test]
fn test_freeze_blocks_state_changing_calls_like_pause() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.propose_emergency_freeze(&t.buyer);

    let shipment_id = sid(&t.env, "s1");
    let result = client.try_create_shipment(
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
    assert!(result.is_err());
}

#[test]
fn test_supermajority_reached_activates_freeze() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // 5 admins, default supermajority 80% => requires 4 approvals.
    let admins: soroban_sdk::Vec<Address> = soroban_sdk::vec![
        &t.env,
        t.buyer.clone(),
        Address::generate(&t.env),
        Address::generate(&t.env),
        Address::generate(&t.env),
        Address::generate(&t.env),
    ];
    client.initialize_multisig_admin(&t.buyer, &admins, &1u32);

    let proposal_id = client.propose_emergency_freeze(&t.buyer);
    assert!(!client.is_emergency_frozen());

    client.approve_emergency_freeze(&admins.get(1).unwrap(), &proposal_id);
    client.approve_emergency_freeze(&admins.get(2).unwrap(), &proposal_id);
    assert!(!client.is_emergency_frozen());

    // Fourth distinct approval (buyer + 3 more) reaches the 80% supermajority.
    client.approve_emergency_freeze(&admins.get(3).unwrap(), &proposal_id);
    assert!(client.is_emergency_frozen());
    assert!(client.get_emergency_freeze_proposal(&proposal_id).is_none());
}

#[test]
fn test_sub_supermajority_approvals_do_not_activate_freeze() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // 5 admins, default supermajority 80% => requires 4 approvals.
    let admins: soroban_sdk::Vec<Address> = soroban_sdk::vec![
        &t.env,
        t.buyer.clone(),
        Address::generate(&t.env),
        Address::generate(&t.env),
        Address::generate(&t.env),
        Address::generate(&t.env),
    ];
    client.initialize_multisig_admin(&t.buyer, &admins, &1u32);

    let proposal_id = client.propose_emergency_freeze(&t.buyer);
    client.approve_emergency_freeze(&admins.get(1).unwrap(), &proposal_id);
    client.approve_emergency_freeze(&admins.get(2).unwrap(), &proposal_id);

    // Only 3 of 5 approved (60%) — below the 80% supermajority.
    assert!(!client.is_emergency_frozen());
    let pending = client
        .get_emergency_freeze_proposal(&proposal_id)
        .expect("proposal must still be pending below supermajority");
    assert_eq!(pending.approvals.len(), 3);
}

#[test]
fn test_custom_supermajority_bps_is_honored() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admin2 = Address::generate(&t.env);
    client.initialize_multisig_admin(
        &t.buyer,
        &soroban_sdk::vec![&t.env, t.buyer.clone(), admin2.clone()],
        &1u32,
    );
    // 100% supermajority required.
    client.set_freeze_supermajority_bps(&t.buyer, &10_000u32);

    let proposal_id = client.propose_emergency_freeze(&t.buyer);
    assert!(!client.is_emergency_frozen());
    client.approve_emergency_freeze(&admin2, &proposal_id);
    assert!(client.is_emergency_frozen());
}

#[test]
fn test_unfreeze_requires_matching_supermajority() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admins: soroban_sdk::Vec<Address> = soroban_sdk::vec![
        &t.env,
        t.buyer.clone(),
        Address::generate(&t.env),
        Address::generate(&t.env),
    ];
    client.initialize_multisig_admin(&t.buyer, &admins, &1u32);
    // 100% supermajority so all 3 admins must act.
    client.set_freeze_supermajority_bps(&t.buyer, &10_000u32);

    let freeze_id = client.propose_emergency_freeze(&t.buyer);
    client.approve_emergency_freeze(&admins.get(1).unwrap(), &freeze_id);
    client.approve_emergency_freeze(&admins.get(2).unwrap(), &freeze_id);
    assert!(client.is_emergency_frozen());

    let unfreeze_id = client.propose_emergency_unfreeze(&t.buyer);
    assert!(client.is_emergency_frozen());
    client.approve_emergency_unfreeze(&admins.get(1).unwrap(), &unfreeze_id);
    assert!(client.is_emergency_frozen());
    client.approve_emergency_unfreeze(&admins.get(2).unwrap(), &unfreeze_id);
    assert!(!client.is_emergency_frozen());
}

#[test]
#[should_panic(expected = "already approved by this admin")]
fn test_duplicate_freeze_approval_from_same_admin_is_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admin2 = Address::generate(&t.env);
    client.initialize_multisig_admin(
        &t.buyer,
        &soroban_sdk::vec![&t.env, t.buyer.clone(), admin2.clone()],
        &1u32,
    );
    client.set_freeze_supermajority_bps(&t.buyer, &10_000u32);

    let proposal_id = client.propose_emergency_freeze(&t.buyer);
    client.approve_emergency_freeze(&t.buyer, &proposal_id);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_non_multisig_admin_cannot_propose_freeze() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.initialize_multisig_admin(&t.buyer, &soroban_sdk::vec![&t.env, t.buyer.clone()], &1u32);
    // t.supplier was never registered as a multisig admin.
    client.propose_emergency_freeze(&t.supplier);
}
