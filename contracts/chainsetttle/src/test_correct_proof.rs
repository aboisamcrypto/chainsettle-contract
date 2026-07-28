#![cfg(test)]

//! In-place proof correction before buyer confirmation/dispute.

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Events as _, vec, String, Symbol, TryFromVal};

fn sid(env: &Env, id: &str) -> String {
    String::from_str(env, id)
}

fn ipfs(env: &Env) -> Symbol {
    Symbol::new(env, "ipfs")
}

fn create_ship(t: &crate::test_common::TestSetup, id: &str) -> String {
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
fn test_correct_proof_success_overwrites_hash_keeps_status() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "corr-ok");

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://wrong-cid"),
        &ipfs(&t.env),
    );

    client.correct_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://fixed-cid"),
        &ipfs(&t.env),
    );

    // events().all() reflects only the most recent top-level call — check before reads.
    let events = t.env.events().all();
    let expected = Symbol::new(&t.env, "proof_corrected");
    let mut found = false;
    for e in events.iter() {
        let topics = e.1.clone();
        if topics.len() >= 1 {
            if let Ok(topic0) = Symbol::try_from_val(&t.env, &topics.get(0).unwrap()) {
                if topic0 == expected {
                    found = true;
                    break;
                }
            }
        }
    }
    assert!(found, "proof_corrected event must be emitted");

    let shipment = client.get_shipment(&shipment_id);
    let m = shipment.milestones.get(0).unwrap();
    assert_eq!(m.status, MilestoneStatus::ProofSubmitted);
    assert_eq!(m.proof_hash, sid(&t.env, "ipfs://fixed-cid"));
    assert_eq!(
        client.get_milestone_proof_type(&shipment_id, &0),
        Some(ipfs(&t.env))
    );
}

#[test]
#[should_panic(expected = "proof type not in whitelist")]
fn test_correct_proof_rejects_non_whitelisted_type() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "corr-wl");

    let allowed = vec![&t.env, ipfs(&t.env)];
    client.set_proof_whitelist(&t.buyer, &shipment_id, &0, &allowed);

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://a"),
        &ipfs(&t.env),
    );

    client.correct_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://b"),
        &Symbol::new(&t.env, "pdf"),
    );
}

#[test]
#[should_panic(expected = "milestone is not in proof submitted status")]
fn test_correct_proof_rejected_after_confirmation() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "corr-post");

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://a"),
        &ipfs(&t.env),
    );
    client.confirm_milestone(&t.buyer, &shipment_id, &0);

    client.correct_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://too-late"),
        &ipfs(&t.env),
    );
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_correct_proof_rejects_non_original_submitter() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "corr-role");

    // Supplier submitted — logistics must not be able to correct.
    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://a"),
        &ipfs(&t.env),
    );

    client.correct_proof(
        &t.logistics,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://b"),
        &ipfs(&t.env),
    );
}

#[test]
#[should_panic(expected = "milestone is not in proof submitted status")]
fn test_correct_proof_rejected_after_dispute() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let shipment_id = create_ship(&t, "corr-disp");

    client.submit_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://a"),
        &ipfs(&t.env),
    );
    client.raise_dispute(&t.buyer, &shipment_id, &0);

    client.correct_proof(
        &t.supplier,
        &shipment_id,
        &0,
        &sid(&t.env, "ipfs://b"),
        &ipfs(&t.env),
    );
}
