# Paddle (payments)

Paddle handles payments and subscriptions for Cmdr. There are two environments:

- **Live**: `https://api.paddle.com` — real customers, real money
- **Sandbox**: `https://sandbox-api.paddle.com` — test data for development
- **Dashboard**: https://vendors.paddle.com (login required)
- **Sandbox dashboard**: https://sandbox-vendors.paddle.com

## API access

Two API keys live in `~/.zshenv`: `PADDLE_LIVE_API_KEY` and `PADDLE_SANDBOX_API_KEY`. See
[CONTRIBUTING.md](../../CONTRIBUTING.md#paddle-access-payments) for setup.

**Gotcha**: Like other env vars, the Bash tool's subshell doesn't always inherit from `~/.zshenv`. Read them from
the file when calling the API directly:

```bash
# For sandbox (use this for development and testing)
PADDLE_KEY=$(grep PADDLE_SANDBOX_API_KEY ~/.zshenv | head -1 | sed 's/export PADDLE_SANDBOX_API_KEY=//' | tr -d '"' | tr -d "'")
PADDLE_API=https://sandbox-api.paddle.com

# For live (use carefully — real customer data)
PADDLE_KEY=$(grep PADDLE_LIVE_API_KEY ~/.zshenv | head -1 | sed 's/export PADDLE_LIVE_API_KEY=//' | tr -d '"' | tr -d "'")
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

**Full API docs**: https://developer.paddle.com/api-reference/overview
