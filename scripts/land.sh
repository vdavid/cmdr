#!/bin/bash
# Lands this worktree's branch on `main` through CI: push, PR, wait for `CI OK`, fast-forward
# `main`, sync local `main`. Safe to re-run at any stage. Docs: `scripts/land/CLAUDE.md`.
#
# Usage: pnpm land [-no-wait] [-squash [-m MESSAGE]] [-record N]
#        pnpm land rulesets [-apply]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}/scripts/land"
exec go run . -root "${ROOT}" "$@"
