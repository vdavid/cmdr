# Vendored SMB test containers

**⚠️ These files are a vendored copy of smb2's consumer test harness. Do NOT edit them directly here — your changes will
be lost the next time we re-vendor.**

## Source of truth

`~/projects-git/vdavid/smb2/tests/docker/consumer/` (GitHub:
https://github.com/vdavid/smb2/tree/main/tests/docker/consumer)

## Why vendored?

These files used to be extracted on-demand by `start.sh` via `cargo run --example smb_compose --features smb-e2e`. That
worked locally but broke CI because the extraction required building the full cmdr crate (with GTK system deps) outside
the Docker container where those deps aren't installed. Vendoring sidesteps the whole dance: the files are here, always.

## How to update (when you bump the smb2 git dep)

1. Bump `smb2` in `apps/desktop/src-tauri/Cargo.toml` (or `Cargo.lock`).
2. Re-vendor the compose files:
   ```bash
   rm -rf apps/desktop/test/smb-servers/.compose
   cp -r ~/projects-git/vdavid/smb2/tests/docker/consumer apps/desktop/test/smb-servers/.compose
   ```
   (Or the equivalent from a checkout of the new rev. The smb2 consumer containers live at `tests/docker/consumer/` in
   the smb2 repo.)
3. Force-rebuild the changed containers so they pick up the new configs:
   ```bash
   docker compose -p smb-consumer -f apps/desktop/test/smb-servers/.compose/docker-compose.yml build --no-cache
   ```
4. Commit the new `.compose/` state alongside the `Cargo.lock` bump.
