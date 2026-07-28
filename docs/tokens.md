# Multi-Token Support (#161)

`create_shipment` takes a `token: Address` parameter and stores it on the
`Shipment` struct. Every payout path in the contract — `confirm_milestone`,
`release_held_payment`, `batch_confirm_milestones`, `claim_auto_confirmation`,
`resolve_dispute`, `resolve_dispute_timeout`, `cancel_shipment`,
`supplier_cancel` — transfers funds through
`token::Client::new(&env, &shipment.token)`, i.e. the token the shipment was
*created* with, never a hardcoded token. This means any Stellar Asset
Contract (SAC) works out of the box: USDC, EURC, and native XLM's SAC
wrapper are all just different `Address` values from the contract's point
of view — no per-asset special-casing exists or is needed.

## Admin-managed whitelist

By default the whitelist (`DataKey::AllowedTokens`) is empty, which means
**open mode**: any SAC token address is accepted. Once the admin adds at
least one token, `create_shipment` only accepts tokens on that list.

| Function | Access | Description |
|---|---|---|
| `add_allowed_token(token)` | admin only | Approve a token address for use in `create_shipment`. |
| `remove_allowed_token(token)` | admin only | Revoke approval for a token address. |
| `get_allowed_tokens()` | public | Returns the current whitelist (empty = open mode). |

Attempting to call `create_shipment` with a token that is not on a
non-empty whitelist panics with `"token is not in the approved whitelist"`.

## Getting the right token address for XLM and EURC

**Do not hardcode a contract address from memory or from an AI assistant's
output.** SAC contract IDs are deterministic — they depend on the network
passphrase (mainnet vs. testnet vs. futurenet) and, for non-native assets,
the issuing account — but a single transposed character sends funds to the
wrong (or a non-existent) contract. Always derive or verify the address
immediately before deploying/configuring, using one of these sources:

1. **Stellar CLI** (recommended — computes the address locally, no trust
   required):
   ```sh
   # Native XLM wrapper contract for a given network
   stellar contract id asset --asset native --network mainnet
   stellar contract id asset --asset native --network testnet

   # A specific issued asset (e.g. Circle's USDC or EURC), once you have
   # confirmed the correct issuer account from Circle's own documentation
   stellar contract id asset --asset USDC:<issuer-account-id> --network mainnet
   stellar contract id asset --asset EURC:<issuer-account-id> --network mainnet
   ```
2. **Circle's official documentation** for the current USDC/EURC issuer
   account IDs on Stellar (these are the "source of truth" — do not copy
   an issuer ID from a third party or an unverified block explorer page).
3. **Stellar Expert / Horizon** to cross-check an address you've already
   derived, not as the primary source.

Once verified, register the address with `add_allowed_token` on the
deployed contract instance for that network.

## Testing

`test_multi_token.rs` exercises the full lifecycle (creation, milestone
payout, cancellation refund) against three independently registered SAC
tokens standing in for USDC, XLM, and EURC, plus the whitelist add/remove/
open-mode behavior. Since the contract has no asset-specific logic, these
simulated tokens exercise exactly the same code paths a real USDC, XLM, or
EURC SAC would.
