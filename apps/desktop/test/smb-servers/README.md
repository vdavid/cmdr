# SMB test servers

Docker SMB containers for local development and E2E testing, provided by smb2's consumer test harness.

## Quick start

```bash
./start.sh         # Start core containers (guest, auth, both, readonly, flaky, slow)
./start.sh minimal # Start just guest + auth
./start.sh all     # Start all 14 containers
./stop.sh          # Stop everything
```

On first run, `start.sh` extracts the Docker Compose files from smb2 into `.compose/` (requires cargo).

See [docs/guides/testing/smb-servers.md](../../../../docs/guides/testing/smb-servers.md) for the full documentation.
