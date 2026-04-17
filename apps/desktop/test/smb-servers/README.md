# SMB test servers

Docker SMB containers for local development and E2E testing, provided by smb2's consumer test harness.

## Quick start

```bash
./start.sh         # Start core containers (guest, auth, both, readonly, flaky, slow)
./start.sh minimal # Start just guest + auth
./start.sh all     # Start all 14 containers
./stop.sh          # Stop everything
```

The Docker Compose files live in `.compose/`. They're **vendored** from smb2's consumer test harness (see
`.compose/VENDORED.md`) — if they're missing or stale after an smb2 bump, follow the re-vendor steps there.

See [docs/guides/testing/smb-servers.md](../../../../docs/guides/testing/smb-servers.md) for the full documentation.
