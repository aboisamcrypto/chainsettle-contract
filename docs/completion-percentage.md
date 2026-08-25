# Milestone Completion Percentage (`get_completion_percentage`)

```
get_completion_percentage(shipment_id: String) -> u32   // read-only, 0..=100
```

Returns how far a shipment has **settled**, as a whole-number percentage derived
from the shipment's milestone **payment weights**. Read-only: it requires no
authorization, mutates no state, and is not gated by the emergency pause. It
panics with `"shipment not found"` when `shipment_id` is unknown (including
shipments already moved to the archive by `archive_shipment`).

Implementation: `ChainSettleContract::get_completion_percentage`
(`contracts/chainsetttle/src/lib.rs`).

---

## 1. Milestone payment weights

Each milestone owns a *weight*: the share of the shipment's `total_amount` that
settling that milestone releases from escrow. Two ways to express weights:

| Source | Field / key | Unit | Validation at `create_shipment` |
|---|---|---|---|
| Default | `Milestone.payment_percent` | whole percent | each `>= MinMilestonePercent` (default `5`) and the set **must sum to exactly `100`**, else panic `"milestone percentages must sum to 100"` / `"InvalidPercentages"` |
| Optional (#160) | `ShipmentOptions.milestone_splits`, persisted as `DataKeyExt::MilestoneSplits(shipment_id)` | basis points | length must equal the milestone count and the set **must sum to exactly `10_000`**, else panic `"InvalidSplitConfiguration"` |

When `milestone_splits` is present it takes precedence; otherwise
`payment_percent` is used. Weights can later be re-cut, but never re-scaled:

* `rebalance_milestones(buyer, shipment_id, new_percents)` — buyer-only, allowed
  only while **every** milestone is still `Pending`; new percents must sum to 100.
* `propose_amendment(...)` — buyer + supplier mutual consent; the resulting set
  must still sum to 100.

So the weight sum is a contract invariant: **100 % (10 000 bps) at all times**.

### Weight → money

`milestone_gross_payment()` is the only place that converts a weight into an
amount:

```text
gross(i) = total_amount * splits_bps(i)      / 10_000    // when MilestoneSplits is set
gross(i) = total_amount * payment_percent(i) / 100       // otherwise
```

Because the weights cover the whole shipment, `Σ gross(i) == total_amount`
(± integer truncation). That equality is what lets a *money* ratio be read as a
*weight* ratio.

---

## 2. The formula

```text
settled = released_amount + total_advanced_amount

pct = 0                                              if total_amount <= 0
pct = 0                                              if settled     <= 0
pct = clamp( settled * 100 / total_amount , 0, 100 )  otherwise      // integer division
```

* The multiplication by `100` happens **before** the division, in `i128`, so no
  precision is lost beyond the final floor.
* The result is clamped into `[0, 100]` so no combination of top-ups, penalties,
  refunds or rebalances can ever return an out-of-range value.

### Numerator: two accumulators, both weight-sized

**`Shipment.released_amount`** — increased by `gross(i)` (minus any late-delivery
penalty) each time milestone `i` definitively leaves escrow:

| Flow | Credited |
|---|---|
| `confirm_milestone` (immediate path) | `gross(i) - late_penalty` |
| `claim_auto_confirmation` (review window elapsed) | `gross(i) - late_penalty` |
| `release_held_payment` (holdback window elapsed) | `gross(i)` |
| `batch_confirm_milestones` | `gross(i)` per index |
| `raise_partial_dispute` | the uncontested `(100 - contested_percent)%` of `gross(i)`, paid out immediately |
| `resolve_dispute` / panel resolution, `approve = true` | the disputed share of `gross(i)` (supplier payout) |
| `resolve_dispute` / panel resolution, `approve = false` on a **partial** dispute | the contested share (refunded to the buyer — still leaves escrow) |
| `resolve_dispute_timeout` with `default_resolution = Supplier` | the disputed share |

Not credited: a **full** dispute rejected in the buyer's favour (`approve = false`)
resets the milestone to `Pending`, and `resolve_dispute_timeout` with
`default_resolution = Buyer` refunds without crediting — in both cases that weight
remains unsettled and can be settled later.

**`Shipment.total_advanced_amount`** — an *in-flight* fraction of a milestone
weight that has already been paid out:

* `approve_advance` adds `gross(i) * requested_percent / 100`
  (`requested_percent <= MaxAdvancePercent`, default `30`);
* `consume_advance_for_milestone` subtracts it again when milestone `i` is
  confirmed, at the same moment the *full* `gross(i)` is added to
  `released_amount`.

Net effect: an approved advance is counted exactly once — first as a partial
weight, then folded into the full milestone weight. No double counting.

### Denominator

`Shipment.total_amount`, read live. `top_up_escrow` increases it (and does **not**
change any weight), so an in-flight percentage is diluted by a top-up.

---

## 3. Worked examples

All examples use `total_amount = 1_000_000_000` and the standard weights
`25 / 50 / 25`.

### 3.1 The happy path

| Step | `released` | `advanced` | `pct` |
|---|---|---|---|
| shipment created | 0 | 0 | **0** |
| confirm milestone 0 (weight 25) | 250_000_000 | 0 | **25** |
| confirm milestone 1 (weight 50) | 750_000_000 | 0 | **75** |
| confirm milestone 2 (weight 25) | 1_000_000_000 | 0 | **100** |

`(250_000_000 * 100) / 1_000_000_000 = 25`, and so on. Each confirmation moves
the reading by exactly that milestone's weight.

### 3.2 Basis-point weights and truncation

`milestone_splits = [3_333, 3_333, 3_334]`:

| Step | `released` | Exact | `pct` |
|---|---|---|---|
| confirm milestone 0 | 333_300_000 | 33.33 % | **33** |
| confirm milestone 1 | 666_600_000 | 66.66 % | **66** |
| confirm milestone 2 | 1_000_000_000 | 100 % | **100** |

Integer division floors, so partial progress rounds **down**; the final
milestone still lands on exactly `100`.

### 3.3 Advance payments

Supplier draws a 20 % advance on milestone 0 (weight 25):

| Step | `released` | `advanced` | `pct` |
|---|---|---|---|
| `approve_advance` (20 % of 250_000_000) | 0 | 50_000_000 | **5** |
| `confirm_milestone(0)` | 250_000_000 | 0 | **25** |

### 3.4 Holdback (payment held after confirmation)

`holdback_ledgers = 100`:

| Step | Milestone status | `released` | `pct` |
|---|---|---|---|
| `confirm_milestone(0)` | `ConfirmedHeld` | 0 | **0** |
| `release_held_payment(0)` after the window | `Confirmed` | 250_000_000 | **25** |

Confirmation alone does not move the number — the payment must actually leave
escrow.

### 3.5 Partial dispute resolved for the buyer

Milestone 0 (weight 25) is partially disputed at `contested_percent = 40`:

| Step | `released` | `pct` |
|---|---|---|
| `raise_partial_dispute(0, 40)` → 60 % paid to supplier | 150_000_000 | **15** |
| `resolve_dispute(0, approve = false)` → contested 40 % refunded to buyer | 250_000_000 | **25** |

The full weight ends up settled even though the buyer got part of the money back:
this query tracks **escrow settlement**, not supplier earnings.

By contrast, a *full* dispute rejected in the buyer's favour sends the milestone
back to `Pending` (proof can be resubmitted) and the reading does **not** move.

### 3.6 Top-up dilutes progress

| Step | `released` | `total_amount` | `pct` |
|---|---|---|---|
| `confirm_milestone(0)` | 250 | 1_000 | **25** |
| `top_up_escrow(+1_000)` | 250 | 2_000 | **12** |

`(250 * 100) / 2_000 = 12.5 → 12`.

---

## 4. What does *not* affect the percentage

* **Fees.** Platform fee (`set_fee_config`, per-shipment override, fee tiers),
  logistics fee (`logistics_fee_bps`), arbiter fee (`arbiter_fee_bps`) and
  referral bonuses are deducted from the *payout*, after the full `gross(i)` has
  been credited to `released_amount`. They never reduce the reading.
* **Early-completion bonuses** (`early_bonus_pool`) are funded separately from
  escrow and are not part of `total_amount`.
* **Dispute bonds** and **supplier collateral** are held outside
  `released_amount` / `total_amount` accounting.
* **Milestone status labels.** `Confirmed`, `ConfirmedHeld` and `Resolved` are not
  counted; only the money movements listed in §2 are. Use
  `get_milestone(shipment_id, i).status` for a status-based view.

Late-delivery penalties *do* shrink the credited amount: a milestone penalised at
`confirm_milestone` / `claim_auto_confirmation` credits `gross(i) - penalty`
(penalty capped at 50 % of `gross(i)`, returned to the buyer), so a shipment
finished entirely late can end below `100`.

Terminal states freeze the reading: `cancel_shipment`, `supplier_cancel` and
`claim_deadline_refund` return the *unsettled* remainder to the buyer without
crediting `released_amount`, so a cancelled or expired shipment keeps whatever
percentage it had reached.

---

## 5. Relationship to other queries

| Query | Meaning |
|---|---|
| `get_completion_percentage(id)` | settled share of the escrow, `0..=100` |
| `get_escrow_balance(id)` | `total_amount - released_amount - total_advanced_amount` — the unsettled remainder |
| `get_milestone(id, i)` | per-milestone weight (`payment_percent`) and status |
| `get_shipment(id)` | raw `total_amount`, `released_amount`, `total_advanced_amount` |

Identity: `get_escrow_balance(id) == total_amount - settled`, therefore
`get_completion_percentage(id) == floor((total_amount - get_escrow_balance(id)) * 100 / total_amount)`
clamped to `[0, 100]`.

---

## 6. Tests

`contracts/chainsetttle/src/test_query.rs` pins every rule documented here:

| Test | Documents |
|---|---|
| `test_get_completion_percentage_fresh_shipment` | fresh shipment reads `0` |
| `test_get_completion_percentage_partial_one_milestone` | one 25 % weight → `25` |
| `test_get_completion_percentage_partial_two_milestones` | 25 % + 50 % → `75` |
| `test_get_completion_percentage_full_completion` | all weights → `100` |
| `test_get_completion_percentage_zero_released` | nothing settled → `0`; small amounts still exact |
| `test_get_completion_percentage_bps_splits_truncates_down` | §3.2 basis-point weights + flooring |
| `test_get_completion_percentage_counts_approved_advance` | §3.3 advances counted once |
| `test_get_completion_percentage_excludes_held_payment` | §3.4 holdback lag |
| `test_get_completion_percentage_counts_buyer_refund_settlement` | §3.5 settlement, not earnings |
| `test_get_completion_percentage_diluted_by_top_up` | §3.6 live denominator |
| `test_get_completion_percentage_unaffected_by_platform_fee` | §4 fees are payout-side |
| `test_get_completion_percentage_small_escrow_exact` | tiny escrows still map weights exactly |
