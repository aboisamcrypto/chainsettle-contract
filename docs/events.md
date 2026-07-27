# ChainSettle Event Schema

This document describes the canonical, structured events emitted by the
`ChainSettleContract` for consumption by off-chain indexers, the backend
event listener, and dispute-resolution/audit tooling. These are emitted
**in addition to** the more granular, per-function events already emitted
throughout the contract (e.g. `held_payment_released`, `partial_dispute_raised`,
`nft_mint_hook`); this document only covers the seven canonical lifecycle
events.

## Format

Every event uses the two-topic form:

```
topics: (Symbol("chainsettle"), Symbol(<event_name>))
data:   Map<Symbol, Val>
```

`event_name` is one of the seven names below. The data payload is a
`Map<Symbol, Val>` keyed by the field names listed in each table — this
keeps the schema stable and self-describing even as new optional fields
are added in the future, without breaking positional-tuple decoders.

All `shipment_id` values are `String`. All monetary amounts (`amount`,
`payout_amount`, `refund_amount`, `total_paid`) are `i128`, denominated in
the shipment's escrow token (the smallest unit, e.g. stroops for a 7-decimal
Stellar asset). All `milestone_index` values are `u32`.

## Events

### ShipmentCreated

Emitted once, at the end of `create_shipment`, after the shipment has been
persisted and the escrow funds have been transferred into the contract.

| Field | Type | Description |
|---|---|---|
| `shipment_id` | `String` | The new shipment's identifier. |
| `buyer` | `Address` | The primary (first) buyer. |
| `supplier` | `Address` | The supplier. |
| `arbiter` | `Address` | The assigned arbiter (or the pool sentinel arbiter). |
| `token` | `Address` | The SAC token address used for escrow. |
| `amount` | `i128` | The total shipment value locked in escrow. |

### MilestoneProofSubmitted

Emitted by `submit_proof`, each time the supplier (or logistics provider)
submits or resubmits proof for a milestone.

| Field | Type | Description |
|---|---|---|
| `shipment_id` | `String` | The shipment identifier. |
| `milestone_index` | `u32` | Index of the milestone proof was submitted for. |
| `proof_hash` | `String` | The submitted proof hash / off-chain reference. |
| `supplier` | `Address` | The shipment's supplier. |

### MilestoneConfirmed

Emitted whenever a milestone's payment is actually released to the
supplier — from `confirm_milestone`, `release_held_payment` (once the
holdback window expires), `batch_confirm_milestones` (once per milestone),
and `claim_auto_confirmation`.

| Field | Type | Description |
|---|---|---|
| `shipment_id` | `String` | The shipment identifier. |
| `milestone_index` | `u32` | Index of the confirmed milestone. |
| `payout_amount` | `i128` | Gross milestone payment released (before fee/logistics/penalty deductions). |

### DisputeOpened

Emitted by `raise_dispute` and `raise_partial_dispute` when a buyer opens a
dispute on a milestone.

| Field | Type | Description |
|---|---|---|
| `shipment_id` | `String` | The shipment identifier. |
| `milestone_index` | `u32` | Index of the disputed milestone. |
| `buyer` | `Address` | The buyer who raised the dispute. |

### DisputeResolved

Emitted by `resolve_dispute` (arbiter-driven) and `resolve_dispute_timeout`
(automatic resolution after `dispute_timeout_seconds` elapses).

| Field | Type | Description |
|---|---|---|
| `shipment_id` | `String` | The shipment identifier. |
| `milestone_index` | `u32` | Index of the resolved milestone. |
| `resolution` | `Symbol` | `"supplier"` if the disputed funds were released to the supplier, `"buyer"` if refunded/withheld from the supplier. |
| `resolver` | `Address` | The arbiter who resolved the dispute (for a timeout resolution, the shipment's designated arbiter). |

### ShipmentCancelled

Emitted by `cancel_shipment` (buyer-initiated), `supplier_cancel`
(supplier-initiated after the buyer response deadline lapses),
`claim_deadline_refund` (deadline-based refund → `Expired`), and
`emergency_recover` (admin emergency recovery).

| Field | Type | Description |
|---|---|---|
| `shipment_id` | `String` | The shipment identifier. |
| `refund_amount` | `i128` | Amount refunded to the buyer — the unconfirmed escrow balance, net of any cancellation fee. Funds already released to the supplier for confirmed milestones are never included or reclaimed. For `AdminEmergencyRecovery`, this is the amount recovered to the admin. |
| `reason` | `Symbol` | Why the shipment ended: `"BuyerCancelled"`, `"SupplierCancelled"`, `"DeadlineRefund"`, or `"AdminEmergencyRecovery"`. Additive third field — older indexers that only read the first two fields remain compatible. The typed `CancellationReason` enum is also persisted on `Shipment.cancellation_reason` (empty `Vec` = unset; one entry = set). |

### ShipmentCompleted

Emitted whenever the last milestone confirmation causes a shipment to
transition to `Completed` — from `confirm_milestone`,
`release_held_payment`, `batch_confirm_milestones`, `claim_auto_confirmation`,
`resolve_dispute`, and `resolve_dispute_timeout`.

| Field | Type | Description |
|---|---|---|
| `shipment_id` | `String` | The shipment identifier. |
| `total_paid` | `i128` | Total amount released to the supplier across all milestones (`Shipment.released_amount` at completion). |
