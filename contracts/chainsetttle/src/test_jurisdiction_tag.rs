// #385 — Jurisdiction/compliance tag per shipment for regulatory filtering.
//
// Verifies the optional `jurisdiction` field set at shipment creation is
// stored immutably on the shipment record and indexed for off-chain
// compliance tooling via `get_shipments_by_jurisdiction`.

#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Address as _, String, Symbol};

#[test]
fn test_shipment_untagged_by_default() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "JUR-001");
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

    assert_eq!(client.get_shipment_jurisdiction(&shipment_id), None);
}

#[test]
fn test_shipment_tagged_with_jurisdiction_at_creation() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let shipment_id = String::from_str(&t.env, "JUR-002");
    let mut opts = default_options(&t.env);
    let us = Symbol::new(&t.env, "US");
    opts.jurisdiction = Some(us.clone());

    client.create_shipment(
        &shipment_id,
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000_000i128,
        &build_milestones(&t.env),
        &opts,
    );

    assert_eq!(client.get_shipment_jurisdiction(&shipment_id), Some(us));
}

#[test]
fn test_get_shipments_by_jurisdiction_filters_correctly() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let us = Symbol::new(&t.env, "US");
    let eu = Symbol::new(&t.env, "EU");

    let mut us_opts = default_options(&t.env);
    us_opts.jurisdiction = Some(us.clone());
    let mut eu_opts = default_options(&t.env);
    eu_opts.jurisdiction = Some(eu.clone());

    let ship_us_1 = String::from_str(&t.env, "JUR-US-1");
    let ship_us_2 = String::from_str(&t.env, "JUR-US-2");
    let ship_eu_1 = String::from_str(&t.env, "JUR-EU-1");

    for (id, opts) in [
        (&ship_us_1, &us_opts),
        (&ship_us_2, &us_opts),
        (&ship_eu_1, &eu_opts),
    ] {
        client.create_shipment(
            id,
            &single_buyer_vec(&t.env, &t.buyer),
            &t.supplier,
            &t.logistics,
            &t.arbiter,
            &t.token_id,
            &1_000_000i128,
            &build_milestones(&t.env),
            opts,
        );
    }

    let us_shipments = client.get_shipments_by_jurisdiction(&us);
    assert_eq!(us_shipments.len(), 2);
    assert!(us_shipments.contains(ship_us_1.clone()));
    assert!(us_shipments.contains(ship_us_2.clone()));

    let eu_shipments = client.get_shipments_by_jurisdiction(&eu);
    assert_eq!(eu_shipments.len(), 1);
    assert!(eu_shipments.contains(ship_eu_1.clone()));
}

#[test]
fn test_get_shipments_by_jurisdiction_empty_for_unused_tag() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let unused = Symbol::new(&t.env, "APAC");
    let result = client.get_shipments_by_jurisdiction(&unused);
    assert_eq!(result.len(), 0);
}
