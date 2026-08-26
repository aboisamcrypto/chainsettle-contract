#![cfg(test)]

//! #405 — Milestone proof size/format validation hook.

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec, TestSetup};
use soroban_sdk::{String, Symbol};

fn sid(env: &Env, id: &str) -> String {
    String::from_str(env, id)
}

fn ipfs(env: &Env) -> Symbol {
    Symbol::new(env, "ipfs")
}

fn create_ship(t: &TestSetup, id: &str) -> String {
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = sid(&t.env, id);
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
    shipment_id
}

#[test]
fn test_default_bounds_are_permissive_and_do_not_reject_existing_formats() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "s1");

    // Shortest and various realistic formats used across the existing test
    // suite — none of these should be rejected by default configuration.
    client.submit_proof(&t.supplier, &shipment_id, &0, &sid(&t.env, "h"), &ipfs(&t.env));
    let milestone = client.get_milestone(&shipment_id, &0);
    assert_eq!(milestone.status, MilestoneStatus::ProofSubmitted);
}

#[test]
fn test_proof_hash_too_short_is_rejected_when_min_len_configured() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "s1");

    client.set_proof_hash_length_bounds(&t.buyer, &5u32, &0u32);

    let result = client.try_submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "abc"),
        &ipfs(&t.env),
    );
    assert!(result.is_err());
}

#[test]
fn test_proof_hash_too_long_is_rejected_when_max_len_configured() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "s1");

    client.set_proof_hash_length_bounds(&t.buyer, &0u32, &10u32);

    let result = client.try_submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "this-hash-is-way-too-long-for-the-cap"),
        &ipfs(&t.env),
    );
    assert!(result.is_err());
}

#[test]
fn test_proof_hash_within_bounds_is_accepted() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "s1");

    client.set_proof_hash_length_bounds(&t.buyer, &3u32, &20u32);

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "QmXyz123"),
        &ipfs(&t.env),
    );
    let milestone = client.get_milestone(&shipment_id, &0);
    assert_eq!(milestone.status, MilestoneStatus::ProofSubmitted);
}

#[test]
fn test_proof_hash_violating_required_prefix_is_rejected() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "s1");

    client.set_proof_hash_required_prefix(&t.buyer, &sid(&t.env, "Qm"));

    let result = client.try_submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://not-a-cid"),
        &ipfs(&t.env),
    );
    assert!(result.is_err());
}

#[test]
fn test_proof_hash_matching_required_prefix_is_accepted() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "s1");

    client.set_proof_hash_required_prefix(&t.buyer, &sid(&t.env, "Qm"));

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "QmXyz123"),
        &ipfs(&t.env),
    );
    let milestone = client.get_milestone(&shipment_id, &0);
    assert_eq!(milestone.status, MilestoneStatus::ProofSubmitted);
}

#[test]
fn test_correct_proof_enforces_same_bounds_as_submit_proof() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "s1");

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "QmValid"),
        &ipfs(&t.env),
    );

    client.set_proof_hash_length_bounds(&t.buyer, &5u32, &0u32);

    let result = client.try_correct_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "abc"),
        &ipfs(&t.env),
    );
    assert!(result.is_err());

    // A conforming correction still succeeds.
    client.correct_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "QmCorrected"),
        &ipfs(&t.env),
    );
    let milestone = client.get_milestone(&shipment_id, &0);
    assert_eq!(
        milestone.proof_hash,
        sid(&t.env, "QmCorrected")
    );
}

#[test]
fn test_get_proof_hash_length_bounds_reflects_configuration() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    assert_eq!(client.get_proof_hash_length_bounds(), (0u32, 0u32));
    client.set_proof_hash_length_bounds(&t.buyer, &2u32, &100u32);
    assert_eq!(client.get_proof_hash_length_bounds(), (2u32, 100u32));
}

#[test]
#[should_panic(expected = "min_len must not exceed max_len")]
fn test_set_proof_hash_length_bounds_rejects_inverted_range() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_proof_hash_length_bounds(&t.buyer, &10u32, &5u32);
}
