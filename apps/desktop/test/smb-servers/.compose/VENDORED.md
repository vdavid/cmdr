# Vendored SMB test containers

**⚠️ These files are a vendored copy of smb2's consumer test harness. Do NOT edit them directly here — your changes will
be lost the next time we re-vendor.**

## Source of truth

`~/projects-git/vdavid/smb2/crates/smb2/src/testing/fixtures/consumer/` (GitHub:
https://github.com/vdavid/smb2/tree/main/crates/smb2/src/testing/fixtures/consumer)

**Synced from:** smb2 v0.27.1 (commit `8b55345`), byte-identical to the published crate's
`src/testing/fixtures/consumer/` apart from the two cmdr-owned files. Keep this line in step with the `smb2` version in
`Cargo.lock`: the published crate ships the fixtures, so
`diff -r ~/.cargo/registry/src/*/smb2-<version>/src/testing/fixtures/consumer apps/desktop/test/smb-servers/.compose`
shows drift against the locked version (expect only `VENDORED.md` and `docker-compose.override.yml`).

## Why vendored?

These files used to be extracted on-demand by `start.sh` via `cargo run --example smb_compose --features smb-e2e`. That
worked locally but broke CI because the extraction required building the full cmdr crate (with GTK system deps) outside
the Docker container where those deps aren't installed. Vendoring sidesteps the whole dance: the files are here, always.

## How to update (when you bump the smb2 git dep)

1. Bump `smb2` in `apps/desktop/src-tauri/Cargo.toml` (or `Cargo.lock`).
2. Re-vendor the compose files, preserving the two cmdr-owned files (`VENDORED.md` and
   `docker-compose.override.yml` live only here, not upstream — without both excludes, `--delete` removes the
   override):
   ```bash
   rsync -a --delete --exclude=VENDORED.md --exclude=docker-compose.override.yml \
       ~/projects-git/vdavid/smb2/crates/smb2/src/testing/fixtures/consumer/ \
       apps/desktop/test/smb-servers/.compose/
   ```
   (Or the equivalent from a checkout of the new rev. The smb2 consumer containers live at `crates/smb2/src/testing/fixtures/consumer/`
   in the smb2 repo — they moved there from `tests/docker/consumer/` in 0.11.4 so the published package excludes `tests/`.)
3. Rebuild AND recreate the changed containers. The check runner's stack lease hashes only the compose files, never a
   vendored build context, so a container running from before the re-vendor keeps serving the old image until you
   replace it. Pass the ports the lease uses (Cmdr's 114xx range, `scripts/check/checks/smb_ports.go`), or the
   recreated container publishes smb2's 104xx defaults:
   ```bash
   cd apps/desktop/test/smb-servers/.compose
   SMB_CONSUMER_UNICODE_PORT=11484 docker compose -p smb-consumer \
       -f docker-compose.yml -f docker-compose.override.yml build --no-cache smb-consumer-unicode
   SMB_CONSUMER_UNICODE_PORT=11484 docker compose -p smb-consumer \
       -f docker-compose.yml -f docker-compose.override.yml up -d --no-deps smb-consumer-unicode
   ```
   (Name every changed service, each with its own `SMB_CONSUMER_*_PORT`.) CI starts fresh, so only local stacks need
   this.
4. Commit the new `.compose/` state alongside the `Cargo.lock` bump.

## Why `.compose/` is excluded from `oxfmt`

`.oxfmtrc.json` lists `apps/desktop/test/smb-servers/.compose/` under `ignorePatterns` so the vendored files stay
byte-for-byte identical to smb2's source of truth. Without that, oxfmt would rewrite YAML quoting on every re-vendor
and create endless diff churn. If you ever add a new file type to the vendored set that needs cmdr-specific
formatting, prefer either fixing it upstream in smb2 or extending the exclusion than reformatting in place.
