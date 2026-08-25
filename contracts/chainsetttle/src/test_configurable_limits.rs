#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::{build_milestones, default_options, setup, single_buyer_vec};
use soroban_sdk::{testutils::Address as _, vec, Env, String};

fn sid(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

fn one_milestone(env: &Env, name: &str, percent: u32) -> Milestone {
    Milestone {
        name: sid(env, name),
        payment_percent: percent,
        proof_hash: sid(env, ""),
        status: MilestoneStatus::Pending,
        release_after_ledger: 0,
        proof_submitted_ledger: None,
        dispute_opened_ledger: None,
        deadline_ledger: 0,
        penalty_bps_per_ledger: 0,
    }
}

fn n_milestones(env: &Env, count: u32) -> soroban_sdk::Vec<Milestone> {
    let mut v = soroban_sdk::Vec::new(env);
    let each = 100 / count;
    let mut total = 0u32;
    for i in 0..count {
        let pct = if i == count - 1 { 100 - total } else { each };
        total += pct;
        v.push_back(one_milestone(env, "M", pct));
    }
    v
}

// ============================================================
// #364 — Configurable maximum milestone count per shipment
// ============================================================

#[test]
fn test_max_milestone_count_default_matches_constant() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    assert_eq!(
        client.get_max_milestone_count(),
        crate::constants::DEFAULT_MAX_MILESTONE_COUNT
    );
}

#[test]
fn test_max_milestone_count_default_allows_existing_behaviour() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    // No admin action taken — creating a shipment with the standard 3-milestone
    // fixture must behave exactly as before.
    client.create_shipment(
        &sid(&t.env, "default-cap"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    assert_eq!(
        client.get_shipment(&sid(&t.env, "default-cap")).status,
        ShipmentStatus::Active
    );
}

#[test]
fn test_admin_can_lower_max_milestone_count() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_max_milestone_count(&t.buyer, &4);
    assert_eq!(client.get_max_milestone_count(), 4);
}

#[test]
fn test_admin_can_raise_max_milestone_count() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_max_milestone_count(&t.buyer, &100);
    assert_eq!(client.get_max_milestone_count(), 100);
}

#[test]
fn test_create_shipment_at_configured_cap_succeeds() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_max_milestone_count(&t.buyer, &4);
    client.create_shipment(
        &sid(&t.env, "at-cap"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000,
        &n_milestones(&t.env, 4),
        &default_options(&t.env),
    );
    assert_eq!(
        client.get_shipment(&sid(&t.env, "at-cap")).milestones.len(),
        4
    );
}

#[test]
fn test_create_shipment_below_configured_cap_succeeds() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_max_milestone_count(&t.buyer, &4);
    client.create_shipment(
        &sid(&t.env, "below-cap"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000,
        &n_milestones(&t.env, 2),
        &default_options(&t.env),
    );
    assert_eq!(
        client
            .get_shipment(&sid(&t.env, "below-cap"))
            .milestones
            .len(),
        2
    );
}

#[test]
#[should_panic(expected = "TooManyMilestones")]
fn test_create_shipment_above_configured_cap_panics() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_max_milestone_count(&t.buyer, &4);
    client.create_shipment(
        &sid(&t.env, "above-cap"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000,
        &n_milestones(&t.env, 5),
        &default_options(&t.env),
    );
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_set_max_milestone_count_requires_admin() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_max_milestone_count(&t.buyer2, &4);
}

// ============================================================
// #362 — Per-token minimum and maximum shipment value limits
// ============================================================

#[test]
fn test_token_without_override_uses_global_bound() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_min_shipment_value(&t.buyer, &1_000);
    assert_eq!(client.get_token_min_shipment_value(&t.token_id), None);
    // Global floor still applies since there's no per-token override.
    client.create_shipment(
        &sid(&t.env, "global-floor-ok"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

#[test]
#[should_panic(expected = "MinShipmentValueNotMet")]
fn test_token_without_override_still_enforces_global_floor() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_min_shipment_value(&t.buyer, &1_000);
    client.create_shipment(
        &sid(&t.env, "global-floor-fail"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &500,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

#[test]
fn test_per_token_min_override_set_and_read() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_min_shipment_value(&t.buyer, &100);
    client.set_token_min_shipment_value(&t.buyer, &t.token_id, &5_000);
    assert_eq!(
        client.get_token_min_shipment_value(&t.token_id),
        Some(5_000)
    );
}

#[test]
#[should_panic(expected = "MinShipmentValueNotMet")]
fn test_per_token_min_override_rejects_value_global_would_accept() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    // Global floor is low, but this token has a much higher per-token floor.
    client.set_min_shipment_value(&t.buyer, &100);
    client.set_token_min_shipment_value(&t.buyer, &t.token_id, &5_000);

    client.create_shipment(
        &sid(&t.env, "token-floor-fail"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

#[test]
#[should_panic(expected = "total amount exceeds maximum shipment value")]
fn test_per_token_max_override_rejects_value_global_would_accept() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_max_shipment_value(&t.buyer, &1_000_000_000);
    client.set_token_max_shipment_value(&t.buyer, &t.token_id, &5_000);
    assert_eq!(
        client.get_token_max_shipment_value(&t.token_id),
        Some(5_000)
    );

    client.create_shipment(
        &sid(&t.env, "token-ceiling-fail"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &10_000,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

#[test]
fn test_two_tokens_with_different_bounds() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let token_admin_2 = soroban_sdk::Address::generate(&t.env);
    let token2_id = t
        .env
        .register_stellar_asset_contract_v2(token_admin_2)
        .address();
    let token2_client = soroban_sdk::token::StellarAssetClient::new(&t.env, &token2_id);
    token2_client.mint(&t.buyer, &10_000_000_000);

    client.set_token_min_shipment_value(&t.buyer, &t.token_id, &1_000);
    client.set_token_min_shipment_value(&t.buyer, &token2_id, &10);

    // token2_id only requires >= 10; 500 must succeed even though token_id
    // would reject it.
    client.create_shipment(
        &sid(&t.env, "tok2-ok"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &token2_id,
        &500,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    assert_eq!(
        client.get_shipment(&sid(&t.env, "tok2-ok")).status,
        ShipmentStatus::Active
    );
}

#[test]
#[should_panic(expected = "MinShipmentValueNotMet")]
fn test_two_tokens_with_different_bounds_first_token_still_enforced() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    let token_admin_2 = soroban_sdk::Address::generate(&t.env);
    let token2_id = t
        .env
        .register_stellar_asset_contract_v2(token_admin_2)
        .address();

    client.set_token_min_shipment_value(&t.buyer, &t.token_id, &1_000);
    client.set_token_min_shipment_value(&t.buyer, &token2_id, &10);

    // token_id requires >= 1_000; 500 must fail.
    client.create_shipment(
        &sid(&t.env, "tok1-fail"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &500,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
}

#[test]
fn test_clear_token_override_falls_back_to_global() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_min_shipment_value(&t.buyer, &100);
    client.set_token_min_shipment_value(&t.buyer, &t.token_id, &5_000);
    client.clear_token_min_shipment_value(&t.buyer, &t.token_id);
    assert_eq!(client.get_token_min_shipment_value(&t.token_id), None);

    // 1_000 is below the old per-token override but above the global floor.
    client.create_shipment(
        &sid(&t.env, "cleared-ok"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000,
        &build_milestones(&t.env),
        &default_options(&t.env),
    );
    assert_eq!(
        client.get_shipment(&sid(&t.env, "cleared-ok")).status,
        ShipmentStatus::Active
    );
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_set_token_min_shipment_value_requires_admin() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_token_min_shipment_value(&t.buyer2, &t.token_id, &1_000);
}

// ============================================================
// #365 — Named milestone template library for reuse across shipments
// ============================================================

#[test]
fn test_save_list_and_get_milestone_template() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let template = vec![
        &t.env,
        one_milestone(&t.env, "Dispatched", 40),
        one_milestone(&t.env, "Delivered", 60),
    ];
    client.save_milestone_template(&t.buyer, &sid(&t.env, "standard"), &template);

    let names = client.list_milestone_templates(&t.buyer);
    assert_eq!(names.len(), 1);
    assert_eq!(names.get(0).unwrap(), sid(&t.env, "standard"));

    let fetched = client.get_milestone_template(&t.buyer, &sid(&t.env, "standard"));
    assert_eq!(fetched.len(), 2);
    assert_eq!(fetched.get(0).unwrap().payment_percent, 40);
    assert_eq!(fetched.get(1).unwrap().payment_percent, 60);
}

#[test]
fn test_create_shipment_from_template_matches_template_structure() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let template = vec![
        &t.env,
        one_milestone(&t.env, "Dispatched", 30),
        one_milestone(&t.env, "In Transit", 30),
        one_milestone(&t.env, "Delivered", 40),
    ];
    client.save_milestone_template(&t.buyer, &sid(&t.env, "3-stage"), &template);

    client.create_shipment_from_template(
        &sid(&t.env, "from-template"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000,
        &sid(&t.env, "3-stage"),
        &default_options(&t.env),
    );

    let shipment = client.get_shipment(&sid(&t.env, "from-template"));
    assert_eq!(shipment.milestones.len(), 3);
    assert_eq!(shipment.milestones.get(0).unwrap().payment_percent, 30);
    assert_eq!(shipment.milestones.get(1).unwrap().payment_percent, 30);
    assert_eq!(shipment.milestones.get(2).unwrap().payment_percent, 40);
    assert_eq!(
        shipment.milestones.get(0).unwrap().name,
        sid(&t.env, "Dispatched")
    );
}

#[test]
fn test_templates_from_different_creators_do_not_collide() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    let template_a = vec![&t.env, one_milestone(&t.env, "A-only", 100)];
    let template_b = vec![&t.env, one_milestone(&t.env, "B-only", 100)];

    client.save_milestone_template(&t.buyer, &sid(&t.env, "shared-name"), &template_a);
    client.save_milestone_template(&t.buyer2, &sid(&t.env, "shared-name"), &template_b);

    let from_buyer = client.get_milestone_template(&t.buyer, &sid(&t.env, "shared-name"));
    let from_buyer2 = client.get_milestone_template(&t.buyer2, &sid(&t.env, "shared-name"));

    assert_eq!(
        from_buyer.get(0).unwrap().name,
        sid(&t.env, "A-only")
    );
    assert_eq!(
        from_buyer2.get(0).unwrap().name,
        sid(&t.env, "B-only")
    );
}

#[test]
#[should_panic(expected = "TemplateNotFound")]
fn test_get_missing_template_panics() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.get_milestone_template(&t.buyer, &sid(&t.env, "does-not-exist"));
}

#[test]
#[should_panic(expected = "TemplateNotFound")]
fn test_create_shipment_from_missing_template_panics() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.create_shipment_from_template(
        &sid(&t.env, "no-template"),
        &single_buyer_vec(&t.env, &t.buyer),
        &t.supplier,
        &t.logistics,
        &t.arbiter,
        &t.token_id,
        &1_000,
        &sid(&t.env, "does-not-exist"),
        &default_options(&t.env),
    );
}

#[test]
#[should_panic(expected = "TooManyMilestones")]
fn test_save_milestone_template_respects_max_count() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_max_milestone_count(&t.buyer, &2);
    client.save_milestone_template(&t.buyer, &sid(&t.env, "too-big"), &n_milestones(&t.env, 3));
}
