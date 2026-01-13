# ADR 014: Payment provider choice

## Status

Accepted

## Summary

We chose Paddle as our payment provider for selling Cmdr licenses. Paddle acts as the Merchant of Record, handling global taxes, invoicing, and compliance, while providing license key generation and a webhook-based integration. This minimizes accounting overhead and lets us focus on the product.

## Context, problem, solution

### Context

Cmdr is a paid desktop application with a 14-day trial. After the trial, users need to purchase a $29 one-time license. We need a payment solution that:

- Handles global payments (users worldwide)
- Manages VAT/GST and tax compliance
- Generates or supports license keys
- Provides aggregate payouts (not per-transaction invoices, to minimize accountant fees)
- Has reasonable fees

### Problem

There are many payment providers with different models:

1. **Payment processors** (Stripe): You're the merchant, you handle invoicing, taxes, and compliance
2. **Merchant of Record (MoR)** providers: They're the legal seller, they handle everything

For a solo developer selling globally, handling VAT registration in 27+ EU countries plus other jurisdictions is impractical. We need an MoR.

### Possible solutions considered

| Provider | Model | Fees | Pros | Cons |
|----------|-------|------|------|------|
| **Stripe** | Payment processor | 2.9% + $0.30 | Lowest fees, most control | You handle taxes and invoicing; 1000 sales = 1000 invoices |
| **Gumroad** | MoR | 10% | Simple, indie-friendly | Highest fees, limited customization |
| **LemonSqueezy** | MoR | 5% + extras | Modern API, good docs | +1.5% non-US, +1% EU payout = ~7.5% total |
| **Paddle** | MoR | 5% + $0.50 | All-inclusive, no hidden fees | Per-transaction flat fee hurts on low prices |
| **Polar** | MoR | 4% + $0.40 | Cheapest base rate | +1.5% non-US, newer/less proven |

### Solution

**Paddle** was chosen because:

1. **All-inclusive pricing**: 5% + $0.50 covers everything — no surprises for non-US cards or EU payouts
2. **Aggregate payouts**: One payout per month = one invoice for accountant
3. **Established reputation**: Widely used by indie software (Sketch, etc.)
4. **License key API**: Built-in support for software licensing
5. **Global tax handling**: They calculate, collect, and remit VAT/GST worldwide

On a $29 sale:
- Paddle: $1.95 fee → $27.05 net
- LemonSqueezy (worst case): $2.18 fee → $26.82 net

At scale (30,000 sales), Paddle saves approximately $7,000/year compared to LemonSqueezy.

## Consequences

### Positive

- Zero tax compliance burden
- Single monthly payout simplifies accounting
- Professional checkout experience
- Global payment methods (cards, PayPal, Apple Pay)
- Handles refunds and chargebacks

### Negative

- 5% + $0.50 is higher than Stripe's 2.9% + $0.30
- Less control over checkout UX (hosted page)
- Dependency on third-party service

### Notes

- We generate our own Ed25519-signed license keys via webhook, giving us full control over the license format
- Paddle's built-in license keys could be used instead, but custom keys allow offline validation
- If Paddle becomes problematic, switching to another MoR (LemonSqueezy, Polar) would require minimal code changes
