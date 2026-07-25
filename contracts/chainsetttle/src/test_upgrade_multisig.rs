// #166 — Contract upgrade mechanism via admin multisig.
//
// Verifies propose_upgrade / approve_upgrade / cancel_upgrade against the same
// MultiAdminConfig (admins + threshold) used by the existing generic multisig
// (initialize_multisig_admin / propose_admin_action), and that the single-key
// `upgrade` function is disabled once that multisig has been configured — so a
// single compromised admin key can no longer push an upgrade unilaterally.
//
// Uses a real uploaded WASM (the contract's own compiled binary) so the upgrade
// actually executes via `update_current_contract_wasm`, the same technique used
// by the pre-existing (currently disabled) test_upgrade.rs.
//
// Pre-requisite: build the contract WASM before running this test:
//   stellar contract build   (from workspace root)
// This produces: target/wasm32v1-none/release/chainsetttle.wasm

#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Address as _, String};

const WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/chainsetttle.wasm"
));

fn sid(env: &Env, id: &str) -> String {
    String::from_str(env, id)
}

#[test]
fn test_threshold_one_executes_upgrade_immediately() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    // t.buyer is already the legacy single admin (set via `client.init(&t.buyer)` in setup()).
    client.initialize_multisig_admin(&t.buyer, &soroban_sdk::vec![&t.env, t.buyer.clone()], &1u32);

    let new_wasm_hash = t.env.deployer().upload_contract_wasm(WASM);
    let proposal_id = client.propose_upgrade(&t.buyer, &new_wasm_hash);

    // Threshold of 1 means the proposer's own approval is already sufficient —
    // the proposal must have executed and been removed immediately.
    assert!(
        client.get_upgrade_proposal(&proposal_id).is_none(),
        "threshold=1 upgrade must execute and clear the proposal immediately"
    );

    // Contract must still be fully functional post-upgrade (same binary).
    let shipment_id = sid(&t.env, "post-upgrade-1");
    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000_000i128,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    assert_eq!(client.get_shipment(&shipment_id).status, ShipmentStatus::Active);
}

#[test]
fn test_threshold_two_stages_until_second_approval() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admin2 = Address::generate(&t.env);
    client.initialize_multisig_admin(
        &t.buyer,
        &soroban_sdk::vec![&t.env, t.buyer.clone(), admin2.clone()],
        &2u32,
    );

    let new_wasm_hash = t.env.deployer().upload_contract_wasm(WASM);
    let proposal_id = client.propose_upgrade(&t.buyer, &new_wasm_hash);

    // Only one of two required approvals so far — must still be pending.
    let pending = client
        .get_upgrade_proposal(&proposal_id)
        .expect("proposal must still be pending after only one approval");
    assert_eq!(pending.approvals.len(), 1);
    assert_eq!(pending.new_wasm_hash, new_wasm_hash);

    client.approve_upgrade(&admin2, &proposal_id);

    // Second distinct approval reaches the threshold — proposal must now be executed/cleared.
    assert!(
        client.get_upgrade_proposal(&proposal_id).is_none(),
        "threshold=2 upgrade must execute once the second admin approves"
    );
}

#[test]
#[should_panic(expected = "already approved by this admin")]
fn test_duplicate_approval_from_same_admin_is_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admin2 = Address::generate(&t.env);
    client.initialize_multisig_admin(
        &t.buyer,
        &soroban_sdk::vec![&t.env, t.buyer.clone(), admin2.clone()],
        &2u32,
    );

    let new_wasm_hash = t.env.deployer().upload_contract_wasm(WASM);
    let proposal_id = client.propose_upgrade(&t.buyer, &new_wasm_hash);

    // t.buyer already approved implicitly by proposing — approving again must panic.
    client.approve_upgrade(&t.buyer, &proposal_id);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_non_admin_cannot_propose_upgrade() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.initialize_multisig_admin(&t.buyer, &soroban_sdk::vec![&t.env, t.buyer.clone()], &1u32);

    let new_wasm_hash = t.env.deployer().upload_contract_wasm(WASM);
    // t.supplier was never registered as a multisig admin.
    client.propose_upgrade(&t.supplier, &new_wasm_hash);
}

#[test]
fn test_cancel_upgrade_removes_pending_proposal() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admin2 = Address::generate(&t.env);
    client.initialize_multisig_admin(
        &t.buyer,
        &soroban_sdk::vec![&t.env, t.buyer.clone(), admin2.clone()],
        &2u32,
    );

    let new_wasm_hash = t.env.deployer().upload_contract_wasm(WASM);
    let proposal_id = client.propose_upgrade(&t.buyer, &new_wasm_hash);
    assert!(client.get_upgrade_proposal(&proposal_id).is_some());

    client.cancel_upgrade(&admin2, &proposal_id);
    assert!(
        client.get_upgrade_proposal(&proposal_id).is_none(),
        "cancel_upgrade must remove the pending proposal"
    );
}

#[test]
#[should_panic(expected = "upgrade proposal not found")]
fn test_approve_after_cancel_fails() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let admin2 = Address::generate(&t.env);
    client.initialize_multisig_admin(
        &t.buyer,
        &soroban_sdk::vec![&t.env, t.buyer.clone(), admin2.clone()],
        &2u32,
    );

    let new_wasm_hash = t.env.deployer().upload_contract_wasm(WASM);
    let proposal_id = client.propose_upgrade(&t.buyer, &new_wasm_hash);
    client.cancel_upgrade(&t.buyer, &proposal_id);

    client.approve_upgrade(&admin2, &proposal_id);
}

#[test]
#[should_panic(expected = "upgrade multisig is configured; use propose_upgrade/approve_upgrade instead")]
fn test_single_key_upgrade_disabled_once_multisig_configured() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.initialize_multisig_admin(&t.buyer, &soroban_sdk::vec![&t.env, t.buyer.clone()], &1u32);

    let new_wasm_hash = t.env.deployer().upload_contract_wasm(WASM);
    // The legacy single-key upgrade path must now be blocked, even for the
    // legitimate admin, forcing use of the multisig-gated propose/approve flow.
    client.upgrade(&t.buyer, &new_wasm_hash);
}
