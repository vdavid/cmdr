# ngrok (tunnels)

ngrok exposes local servers to the internet — useful for testing webhooks (for example, Paddle) against a local
API server instance.

- **Dashboard**: https://dashboard.ngrok.com
- **API base**: `https://api.ngrok.com`

## API access

`NGROK_API_KEY` lives in `~/.zshenv`. See [CONTRIBUTING.md](../../CONTRIBUTING.md#ngrok-access-tunnels) for setup.

```bash
# Read the key (Bash tool subshells don't inherit from .zshenv)
NGROK_KEY=$(grep NGROK_API_KEY ~/.zshenv | head -1 | sed 's/export NGROK_API_KEY=//' | tr -d '"' | tr -d "'")

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
