# Paddle (payments)

Paddle handles payments and subscriptions for Cmdr. There are two environments:

- **Live**: `https://api.paddle.com` — real customers, real money
- **Sandbox**: `https://sandbox-api.paddle.com` — test data for development
- **Dashboard**: https://vendors.paddle.com (login required)
- **Sandbox dashboard**: https://sandbox-vendors.paddle.com

## API access

Two API keys are stored in macOS Keychain: `PADDLE_LIVE_API_KEY` and `PADDLE_SANDBOX_API_KEY`. See
[CONTRIBUTING.md](../../CONTRIBUTING.md#paddle-access-payments) for setup.

```bash
# For sandbox (use this for development and testing)
PADDLE_KEY=$(security find-generic-password -a "$USER" -s "PADDLE_SANDBOX_API_KEY" -w)
PADDLE_API=https://sandbox-api.paddle.com

# For live (use carefully — real customer data)
PADDLE_KEY=$(security find-generic-password -a "$USER" -s "PADDLE_LIVE_API_KEY" -w)
PADDLE_API=https://api.paddle.com
```

## Common API operations

```bash
# List products
curl -s "${PADDLE_API}/products" \
  -H "Authorization: Bearer ${PADDLE_KEY}" | jq '.data[] | {id, name, status}'

# List active subscriptions
curl -s "${PADDLE_API}/subscriptions?status=active" \
  -H "Authorization: Bearer ${PADDLE_KEY}" | jq '.data[] | {id, status, customer_id, current_billing_period}'

# Get a specific subscription
curl -s "${PADDLE_API}/subscriptions/{subscription_id}" \
  -H "Authorization: Bearer ${PADDLE_KEY}" | jq '.data'

# List customers
curl -s "${PADDLE_API}/customers?per_page=5" \
  -H "Authorization: Bearer ${PADDLE_KEY}" | jq '.data[] | {id, name, email}'

# List transactions (payments)
curl -s "${PADDLE_API}/transactions?per_page=5" \
  -H "Authorization: Bearer ${PADDLE_KEY}" | jq '.data[] | {id, status, customer_id, totals}'
```

**Gotcha**: Checkout settings (default payment link, etc.) have no API — they can only be changed in the dashboard:
[sandbox](https://sandbox-vendors.paddle.com/checkout-settings) | [live](https://vendors.paddle.com/checkout-settings).

**Full API docs**: https://developer.paddle.com/api-reference/overview
