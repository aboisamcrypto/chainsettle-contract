# New Features Usage Guide

## Quick Reference for Treasury Management & Buyer Reliability Features

---

## 1. Query Treasury Revenue

### Function Signature

```rust
pub fn get_treasury_revenue(env: Env, token: Address) -> i128
```

### Usage Example

```rust
let usdc_address = Address::from_string("GDGQ...");
let revenue = client.get_treasury_revenue(&usdc_address);
// Returns cumulative fees collected in USDC
```

### Notes

- Read-only, no authentication required
- Returns 0 for tokens with no fee history
- Tracks ALL fees collected, across all shipments
- Counter never resets (cumulative forever)

---

## 2. Withdraw Treasury Dust

### Function Signature

```rust
pub fn withdraw_treasury_dust(
    env: Env,
    admin: Address,
    token: Address,
    to: Address,
) -> i128
```

### Usage Example

```rust
let admin = Address::from_string("ADMIN...");
let usdc_address = Address::from_string("GDGQ...");
let treasury_wallet = Address::from_string("TREAS...");

let withdrawn = client.withdraw_treasury_dust(
    &admin,
    &usdc_address,
    &treasury_wallet,
);
// Returns amount of dust withdrawn
```

### Safety Guarantees

- Only withdraws: `contract_balance - total_escrowed`
- Active shipment funds are **never touched**
- Panics with "no dust available" if balance ≤ escrowed amount

### When to Use

- After multiple shipments have completed
- When small rounding remainders accumulate
- Before contract upgrade/migration
- Periodic treasury sweeps

### Event Emitted

```rust
topics: ("treasury_withdrawal", token_address)
data: (recipient_address, amount, ledger_sequence)
```

---

## 3. Configure Multi-Recipient Fee Distribution

### Function Signature

```rust
pub fn set_fee_recipients(
    env: Env,
    admin: Address,
    recipients: Vec<FeeRecipient>,
)
```

### FeeRecipient Structure

```rust
pub struct FeeRecipient {
    pub recipient: Address,
    pub share_bps: u32,  // basis points
}
```

### Usage Example: 70/30 Split

```rust
let admin = Address::from_string("ADMIN...");
let dao_treasury = Address::from_string("DAO...");
let contributor_pool = Address::from_string("CONTRIB...");

let recipients = vec![
    &env,
    FeeRecipient {
        recipient: dao_treasury,
        share_bps: 7000,  // 70%
    },
    FeeRecipient {
        recipient: contributor_pool,
        share_bps: 3000,  // 30%
    },
];

client.set_fee_recipients(&admin, &recipients);
```

### Usage Example: 3-Way Split (50/30/20)

```rust
let recipients = vec![
    &env,
    FeeRecipient {
        recipient: main_treasury,
        share_bps: 5000,  // 50%
    },
    FeeRecipient {
        recipient: dev_fund,
        share_bps: 3000,  // 30%
    },
    FeeRecipient {
        recipient: marketing_fund,
        share_bps: 2000,  // 20%
    },
];

client.set_fee_recipients(&admin, &recipients);
```

### Validation Rules

✅ Shares must sum to **exactly 10,000** (100%)  
✅ Recipients list cannot be empty  
✅ Admin-only function

### Rounding Behavior

- First recipient receives any remainder from integer division
- Example: 1000 fee split 33/33/34 → 330 + 330 + 340 = 1000

### Backward Compatibility

To revert to single treasury:

```rust
let recipients = vec![
    &env,
    FeeRecipient {
        recipient: single_treasury,
        share_bps: 10000,  // 100%
    },
];
client.set_fee_recipients(&admin, &recipients);
```

### Event Emitted

```rust
topics: ("fee_recipients_set",)
data: (recipient_count, ledger_sequence)
```

---

## 4. Query Buyer Reliability

### Function Signature

```rust
pub fn get_buyer_reliability(env: Env, buyer: Address) -> BuyerReliability
```

### BuyerReliability Structure

```rust
pub struct BuyerReliability {
    pub total_confirmations: u32,
    pub total_confirmation_latency: u64,
    pub disputes_lost: u32,
    pub disputes_total: u32,
}
```

### Usage Example

```rust
let buyer_address = Address::from_string("BUYER...");
let reliability = client.get_buyer_reliability(&buyer_address);

// Calculate metrics
let avg_latency = if reliability.total_confirmations > 0 {
    reliability.total_confirmation_latency / reliability.total_confirmations as u64
} else {
    0
};

let dispute_loss_rate = if reliability.disputes_total > 0 {
    (reliability.disputes_lost * 100) / reliability.disputes_total
} else {
    0
};

println!("Average confirmation time: {} ledgers", avg_latency);
println!("Dispute loss rate: {}%", dispute_loss_rate);
println!("Total transactions: {}", reliability.total_confirmations);
```

### Interpretation

#### Confirmation Latency

- **Lower is better** for suppliers
- Measured in ledgers (≈5 seconds each on Stellar)
- Example: 100 ledgers ≈ 8.3 minutes average response time

#### Dispute Loss Rate

- **Higher is worse** for buyer reputation
- `disputes_lost / disputes_total * 100%`
- Example: 2 lost out of 10 = 20% loss rate

#### Transaction Volume

- `total_confirmations` = number of milestones confirmed
- Higher count = more experience/history

### When to Use

**Suppliers deciding whether to accept a shipment:**

```rust
let reliability = client.get_buyer_reliability(&proposed_buyer);

if reliability.total_confirmations < 5 {
    // New buyer, proceed with caution
}

let avg_latency = reliability.total_confirmation_latency
                  / reliability.total_confirmations as u64;
if avg_latency > 1000 {  // > ~83 minutes average
    // Slow buyer, may want higher dispute_bond_amount
}

if reliability.disputes_total > 0 {
    let loss_rate = (reliability.disputes_lost * 100) / reliability.disputes_total;
    if loss_rate > 50 {
        // Buyer loses most disputes, high risk
    }
}
```

### Default Values

New buyers with no history return:

```rust
BuyerReliability {
    total_confirmations: 0,
    total_confirmation_latency: 0,
    disputes_lost: 0,
    disputes_total: 0,
}
```

---

## Complete Integration Example

### Scenario: Protocol with DAO + Dev Fund Split

```rust
// 1. Admin configures 60/40 fee split
let admin = Address::from_string("ADMIN...");
let dao = Address::from_string("DAO...");
let dev_fund = Address::from_string("DEV...");

client.set_fee_recipients(
    &admin,
    &vec![
        &env,
        FeeRecipient { recipient: dao, share_bps: 6000 },
        FeeRecipient { recipient: dev_fund, share_bps: 4000 },
    ],
);

// 2. Query accumulated revenue
let usdc = Address::from_string("USDC...");
let total_revenue = client.get_treasury_revenue(&usdc);
println!("Total protocol revenue: {}", total_revenue);

// 3. Supplier checks buyer before accepting shipment
let buyer = Address::from_string("BUYER...");
let reliability = client.get_buyer_reliability(&buyer);

if reliability.total_confirmations < 3 {
    // Require higher collateral for new buyers
    opts.dispute_bond_amount = 1_000_000;  // 1 USDC
}

// 4. After shipment completes, fees automatically split 60/40
// DAO receives 60% of fee
// Dev fund receives 40% of fee
// Buyer reliability score updated with confirmation latency

// 5. Periodically sweep dust
let dust = client.withdraw_treasury_dust(
    &admin,
    &usdc,
    &dao,
);
println!("Swept dust: {}", dust);
```

---

## Migration from Single Treasury

### Before (Existing Code)

```rust
// Old way - single treasury
client.set_fee_config(
    &admin,
    &200,  // 2% fee
    &treasury,
);
```

### After (Multi-Recipient)

```rust
// New way - same behavior with multiple recipients
client.set_fee_config(
    &admin,
    &200,  // 2% fee (unchanged)
    &treasury,  // Still works as primary
);

// When ready to split fees:
client.set_fee_recipients(
    &admin,
    &vec![
        &env,
        FeeRecipient { recipient: treasury_1, share_bps: 5000 },
        FeeRecipient { recipient: treasury_2, share_bps: 5000 },
    ],
);
```

**No breaking changes** - existing code continues to work!

---

## Monitoring & Analytics

### Track Revenue Growth

```rust
// Periodic revenue snapshots
let snapshot = |token: Address| {
    let revenue = client.get_treasury_revenue(&token);
    log_metric("protocol_revenue", revenue, token);
};

// Call weekly/monthly
snapshot(usdc_address);
snapshot(xlm_address);
```

### Buyer Risk Scoring

```rust
fn calculate_risk_score(reliability: BuyerReliability) -> u32 {
    if reliability.total_confirmations == 0 {
        return 100;  // New buyer = high risk
    }

    let avg_latency = reliability.total_confirmation_latency
                      / reliability.total_confirmations as u64;
    let latency_score = (avg_latency / 100).min(50) as u32;

    let dispute_score = if reliability.disputes_total > 0 {
        (reliability.disputes_lost * 50) / reliability.disputes_total
    } else {
        0
    };

    latency_score + dispute_score  // 0-100 scale
}
```

---

## Error Handling

### Common Errors

#### withdraw_treasury_dust

```rust
// Panics: "unauthorized"
// → Caller is not admin

// Panics: "no dust available"
// → contract_balance ≤ total_escrowed
```

#### set_fee_recipients

```rust
// Panics: "unauthorized"
// → Caller is not admin

// Panics: "recipients cannot be empty"
// → recipients vec is empty

// Panics: "fee shares must sum to exactly 10000"
// → Sum of share_bps != 10000
```

---

## Performance Considerations

### Gas Costs

- **get_treasury_revenue**: 1 storage read (cheap)
- **withdraw_treasury_dust**: 1 storage read + 1 token transfer (moderate)
- **set_fee_recipients**: 1 storage write (cheap)
- **get_buyer_reliability**: 1 storage read (cheap)

### Fee Distribution Overhead

- Single recipient: Same as before (no overhead)
- Multiple recipients: +1 transfer per additional recipient
- Example: 3-way split = 2 extra transfers (~6000 extra ops)

### Storage Growth

- Revenue: 1 entry per token (minimal)
- Fee recipients: 1 entry total (minimal)
- Buyer reliability: 1 entry per unique buyer (scales linearly)

---

## Best Practices

### For Protocol Admins

1. Configure fee recipients **once** at deployment
2. Use `get_treasury_revenue()` for accounting/reports
3. Schedule weekly/monthly dust sweeps
4. Monitor fee distribution to all recipients

### For Suppliers

1. Query `get_buyer_reliability()` before accepting shipments
2. Set higher collateral/bonds for low-reliability buyers
3. Consider refusing buyers with >80% dispute loss rate
4. Check buyer history (total_confirmations) for experience

### For Integrators

1. Display buyer reliability scores in UI
2. Show estimated fee split to users
3. Add revenue tracking to dashboards
4. Implement automated dust sweeping bots

---

## Questions & Support

### Where does revenue tracking start?

- Tracking begins **now** (after this deployment)
- Historical fees are not retroactively counted
- Counter starts at 0 for each token

### What happens to old shipments?

- Nothing changes for existing shipments
- They continue using old fee config (single treasury)
- New shipments automatically use new multi-recipient split

### Can I disable multi-recipient split?

- Yes, set a single recipient with share_bps = 10000
- Or continue using `set_fee_config()` only

### Does buyer reliability affect shipment execution?

- No, it's purely informational
- Suppliers can use it for risk assessment
- No automatic blocking or restrictions
