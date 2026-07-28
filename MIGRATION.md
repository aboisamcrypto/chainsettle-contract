# ChainSettle Migration Guide

## Issue #137 — String → BytesN<32> Shipment ID Migration

### Overview

Issue #137 proposes replacing the `String`-typed `shipment_id` with `BytesN<32>` throughout
the contract to eliminate variable-length storage overhead, improve key collision resistance,
and align with Soroban identifier best practices.

**Status:** Deferred — issue [#137](https://github.com/shakurJJ/chainsettle-contract/issues/137)
was closed by documenting this migration path only ([PR #157](https://github.com/shakurJJ/chainsettle-contract/pull/157));
the `String` → `BytesN<32>` type change was **not** implemented. `lib.rs` still uses `String`
for `shipment_id` everywhere. This guide remains the plan for a future upgrade that actually
applies the refactor; closing #137 did not mean the migration shipped.

---

### Why BytesN<32>

| Concern | String | BytesN<32> |
|---|---|---|
| Storage cost | Variable (1 byte per char + length prefix) | Fixed 32 bytes always |
| Key collision surface | Larger (prefix attacks possible) | Minimal (fixed-width hash-like key) |
| Soroban best practice | Acceptable for short keys | Preferred for identifiers |
| Ledger footprint | Higher on long IDs | Constant regardless of content |

---

### Contract Changes Required

1. `Shipment.id` type: `String` → `BytesN<32>`
2. All public function parameters `shipment_id: String` → `shipment_id: BytesN<32>`
3. `DataKey` variants holding String shipment IDs → `BytesN<32>`:
   - `Shipment(String)` → `Shipment(BytesN<32>)`
   - `CancelPolicy(String)` → `CancelPolicy(BytesN<32>)`
   - `ProofSubmittedAt(String, u32)` → `ProofSubmittedAt(BytesN<32>, u32)`
   - `Amendment(String, u32)` → `Amendment(BytesN<32>, u32)`
   - `ArbiterRotation(String)` → `ArbiterRotation(BytesN<32>)`
   - `AdvanceRequest(String, u32)` → `AdvanceRequest(BytesN<32>, u32)`
   - `MilestoneProofWhitelist(String, u32)` → `MilestoneProofWhitelist(BytesN<32>, u32)`
   - `SubmittedProofType(String, u32)` → `SubmittedProofType(BytesN<32>, u32)`
   - `DisputeContestedPercent(String, u32)` → `DisputeContestedPercent(BytesN<32>, u32)`
   - `SupplierCollateral(String)` → `SupplierCollateral(BytesN<32>)`
4. `DisputeEntry.shipment_id`: `String` → `BytesN<32>`
5. `BatchShipmentParams.shipment_id`: `String` → `BytesN<32>`
6. All index lists (`AllShipments`, `SupplierShipments`, `BuyerShipments`, `ShipmentsByStatus`):
   `Vec<String>` → `Vec<BytesN<32>>`
7. `storage.rs` versioned keys (`V1Shipment`, etc.) updated in lockstep

---

### Client-Facing Change

Callers currently pass a UTF-8 string as shipment ID (e.g. `"SHIP-001"`).
After this migration, callers pass a 32-byte value — typically a SHA-256 hash of their
canonical shipment identifier:

```typescript
// Before
const shipmentId = "SHIP-001";

// After — hash the canonical ID client-side
const shipmentId = sha256("SHIP-001"); // 32 bytes
```

A zero-padded UTF-8 encoding also works for short IDs (≤32 chars):

```typescript
// UTF-8 + zero-pad for short IDs
const buf = Buffer.alloc(32);
Buffer.from("SHIP-001").copy(buf);
const shipmentId = buf; // BytesN<32>
```

---

### On-Chain State Migration

Because this changes the storage keys, **existing shipments stored under `String` keys
will not be readable under `BytesN<32>` keys**.

The migration procedure is:

1. Deploy upgraded contract with `migrate()` function that:
   - Reads all shipment IDs from `AllShipments` (still stored as `Vec<String>`)
   - For each ID, reads the `V1Shipment(String)` entry
   - Derives the `BytesN<32>` key (zero-padded UTF-8 of the original string ID)
   - Writes to `V1Shipment(BytesN<32>)` with the same `Shipment` value (`.id` field updated)
   - Removes the old `V1Shipment(String)` entry
   - Repeats for all related keys (CancelPolicy, ProofSubmittedAt, etc.)
2. Call `migrate()` once immediately after deployment.
3. Migration is idempotent — safe to call multiple times.

```rust
// Pseudocode in migrate()
pub fn migrate(env: Env) {
    let ids: Vec<String> = storage::get_all_shipments_legacy(&env);
    for id in ids.iter() {
        let key_new = string_to_bytes32(&env, &id);
        if let Some(mut shipment) = env.storage().persistent().get(&DataKey::V1Shipment(id.clone())) {
            shipment.id = key_new.clone();
            env.storage().persistent().set(&DataKey::V1Shipment(key_new.clone()), &shipment);
            env.storage().persistent().remove(&DataKey::V1Shipment(id.clone()));
            // ... migrate related keys
        }
    }
}
```

---

### Test Updates

All tests need to replace:

```rust
// Before
let shipment_id = String::from_str(&env, "SHIP-001");
```

with:

```rust
// After — using the test helper
fn shipment_id_bytes(env: &Env, s: &str) -> BytesN<32> {
    let b = s.as_bytes();
    assert!(b.len() <= 32);
    let mut arr = [0u8; 32];
    arr[..b.len()].copy_from_slice(b);
    BytesN::from_array(env, &arr)
}

let shipment_id = shipment_id_bytes(&env, "SHIP-001");
```
