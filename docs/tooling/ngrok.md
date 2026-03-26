# ngrok (tunnels)

ngrok exposes local servers to the internet — useful for testing webhooks (for example, Paddle) against a local
API server instance.

- **Dashboard**: https://dashboard.ngrok.com
- **API base**: `https://api.ngrok.com`

## API access

`NGROK_API_KEY` is stored in macOS Keychain. See [CONTRIBUTING.md](../../CONTRIBUTING.md#ngrok-access-tunnels) for setup.

```bash
NGROK_KEY=$(security find-generic-password -a "$USER" -s "NGROK_API_KEY" -w)

# List active endpoints
curl -s "https://api.ngrok.com/endpoints" \
  -H "Authorization: Bearer ${NGROK_KEY}" \
  -H "Ngrok-Version: 2" | jq '.endpoints[]'

# List active tunnels
curl -s "https://api.ngrok.com/tunnels" \
  -H "Authorization: Bearer ${NGROK_KEY}" \
  -H "Ngrok-Version: 2" | jq '.tunnels[]'
```

**Full API docs**: https://ngrok.com/docs/api/
