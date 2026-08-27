// #387 — Configurable maximum allowed-token list size.
//
// Verifies `add_allowed_token` respects the admin-configured cap so the
// whitelist can no longer grow unboundedly once a limit is set.

#![cfg(test)]

extern crate std;

use super::*;
use crate::test_common::setup;
use soroban_sdk::testutils::Address as _;

#[test]
fn test_max_allowed_tokens_defaults_to_uncapped() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    assert_eq!(client.get_max_allowed_tokens(), 0);

    for _ in 0..10 {
        let token = Address::generate(&t.env);
        client.add_allowed_token(&token);
    }
    assert_eq!(client.get_allowed_tokens().len(), 10);
}

#[test]
fn test_admin_can_configure_max_allowed_tokens() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.set_max_allowed_tokens(&t.buyer, &2u32);
    assert_eq!(client.get_max_allowed_tokens(), 2);
}

#[test]
fn test_add_allowed_token_rejects_once_cap_reached() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.set_max_allowed_tokens(&t.buyer, &2u32);

    client.add_allowed_token(&Address::generate(&t.env));
    client.add_allowed_token(&Address::generate(&t.env));
    assert_eq!(client.get_allowed_tokens().len(), 2);

    let result = client.try_add_allowed_token(&Address::generate(&t.env));
    assert!(result.is_err());
    assert_eq!(client.get_allowed_tokens().len(), 2);
}

#[test]
fn test_raising_cap_allows_further_additions() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);

    client.set_max_allowed_tokens(&t.buyer, &1u32);
    client.add_allowed_token(&Address::generate(&t.env));
    assert!(client.try_add_allowed_token(&Address::generate(&t.env)).is_err());

    client.set_max_allowed_tokens(&t.buyer, &2u32);
    client.add_allowed_token(&Address::generate(&t.env));
    assert_eq!(client.get_allowed_tokens().len(), 2);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_only_admin_can_set_max_allowed_tokens() {
    let t = setup();
    let client = ChainSettleContractClient::new(&t.env, &t.contract_id);
    client.set_max_allowed_tokens(&t.supplier, &5u32);
}
