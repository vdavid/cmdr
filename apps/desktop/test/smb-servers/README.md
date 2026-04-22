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

CI runs the Rust SMB integration tests automatically via the `desktop-rust-integration-tests` check, which starts the
`core` containers, runs `cargo nextest run --run-ignored only -E 'test(smb_integration_)'`, and tears them down.
Locally, `./scripts/check.sh --rust` includes the same check.

See [docs/guides/testing/smb-servers.md](../../../../docs/guides/testing/smb-servers.md) for the full documentation.
