#!/bin/bash
set -e

# Run the check tool using go run. `go run .` (not `go run *.go`) so the
# package's _test.go files don't break the invocation.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_DIR="${SCRIPT_DIR}/check"

cd "${CHECK_DIR}"
exec go run . "$@"
