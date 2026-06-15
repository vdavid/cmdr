# Desktop app tests details

Depth and rationale. `CLAUDE.md` holds the must-knows; the case study lives here.

## Case study: the `sleep 3` flake (2026-05-14)

`smb-servers/start.sh` had `sleep 3` after `docker compose up -d`. The `guest` container bound port 445 fast enough;
`auth`, `50shares`, and `unicode` legitimately needed >3 s under load to finish user creation / share materialisation.
E2E runs flaked with `Cannot reach smb-consumer-X` because smbd hadn't bound the port yet when tests started connecting.
Replaced with per-service TCP probes on the published `445` port: now exits in ~100 ms on a warm machine, in 5-10 s on a
cold one, and gives a deterministic `did not accept TCP within 60s` error if a container is genuinely broken. This is
the concrete instance behind the "never use magic timer waits" must-know in `CLAUDE.md`. See also `docs/testing.md` §
"Bare `await pollUntil(...)` in E2E specs".

## Linux Docker infrastructure

`e2e-linux/docker/` holds a content-hash-cached `Dockerfile.base` (system layer) + a thin `Dockerfile` + entrypoint. The
`e2e-linux.sh` script builds the Tauri binary with `--features playwright-e2e,virtual-mtp` inside Docker, launches it,
and runs the Playwright tests.
