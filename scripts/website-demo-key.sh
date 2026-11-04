#!/bin/bash
# Prints the content key of the browser demo's sources at a commit (default HEAD): a hash of every
# file the demo bundle builds from, so the same sources always get the same key. It tags the demo
# image (`ghcr.io/vdavid/cmdr-demo:<key>`) CI publishes, and the website deploy computes it from
# its own checkout to pick that image, so a site is only ever built with the demo of its own
# sources. `apps/website/DETAILS.md` § Browser demo says why a content key and not a commit SHA.
#
# The path list must equal `websiteDemoInputs` in `scripts/check/checks/inputs.go` (a test there
# holds them together), and ci.yml's `website` filter must cover it: a path hashed here but not in
# the filter changes the key on a commit that publishes no image. The agent docs are left out for
# the same reason (the filter excludes them).
#
# Usage: ./scripts/website-demo-key.sh [rev]   (needs only git, so the deploy server can run it)
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
rev="${1:-HEAD}"

paths=(
  package.json
  pnpm-lock.yaml
  feature-status.json
  apps/desktop/src
  apps/desktop/static
  apps/desktop/package.json
  apps/desktop/vite.config.js
  apps/desktop/tsconfig.json
  apps/website/demo-image.Dockerfile
  crates/cmdr-listing-wasm
  crates/cmdr-listing
  crates/cmdr-fs
  crates/cmdr-collation
  crates/cmdr-brief-columns
  crates/cmdr-menu
  vendor
  scripts/build-listing-wasm.sh
  scripts/build-listing-wasm
  scripts/website-demo-key.sh
  Cargo.toml
  Cargo.lock
  rust-toolchain.toml
)

git ls-tree -r "$rev" -- "${paths[@]}" | grep -Ev '/(CLAUDE|DETAILS)\.md$' | git hash-object --stdin
