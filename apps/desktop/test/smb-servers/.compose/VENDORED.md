# Vendored SMB test containers

**⚠️ These files are a vendored copy of smb2's consumer test harness. Do NOT edit them directly here — your changes will
be lost the next time we re-vendor.**

## Source of truth

`~/projects-git/vdavid/smb2/src/testing/fixtures/consumer/` (GitHub:
https://github.com/vdavid/smb2/tree/main/src/testing/fixtures/consumer)

## Why vendored?

These files used to be extracted on-demand by `start.sh` via `cargo run --example smb_compose --features smb-e2e`. That
worked locally but broke CI because the extraction required building the full cmdr crate (with GTK system deps) outside
the Docker container where those deps aren't installed. Vendoring sidesteps the whole dance: the files are here, always.

## How to update (when you bump the smb2 git dep)

1. Bump `smb2` in `apps/desktop/src-tauri/Cargo.toml` (or `Cargo.lock`).
2. Re-vendor the compose files, preserving this `VENDORED.md` (it lives only here, not upstream):
   ```bash
   rsync -a --delete --exclude=VENDORED.md \
       ~/projects-git/vdavid/smb2/src/testing/fixtures/consumer/ \
       apps/desktop/test/smb-servers/.compose/
   ```
   (Or the equivalent from a checkout of the new rev. The smb2 consumer containers live at `src/testing/fixtures/consumer/`
   in the smb2 repo — they moved there from `tests/docker/consumer/` in 0.11.4 so the published package excludes `tests/`.)
3. Force-rebuild the changed containers so they pick up the new configs:
   ```bash
   docker compose -p smb-consumer -f apps/desktop/test/smb-servers/.compose/docker-compose.yml build --no-cache
   ```
4. Commit the new `.compose/` state alongside the `Cargo.lock` bump.

## Why `.compose/` is excluded from `oxfmt`

`.oxfmtrc.json` lists `apps/desktop/test/smb-servers/.compose/` under `ignorePatterns` so the vendored files stay
byte-for-byte identical to smb2's source of truth. Without that, oxfmt would rewrite YAML quoting on every re-vendor
and create endless diff churn. If you ever add a new file type to the vendored set that needs cmdr-specific
formatting, prefer either fixing it upstream in smb2 or extending the exclusion than reformatting in place.
