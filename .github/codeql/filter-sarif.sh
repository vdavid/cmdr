#!/bin/bash
# Usage: filter-sarif.sh <in.sarif> <out.sarif>
#
# Drops the findings `accepted-findings.json` lists (by rule id and file), writes the rest to
# <out.sarif> for upload, and exits 1 if any remaining finding blocks: a security query at high or
# critical (security-severity >= 7.0, GitHub's cut-off for "high"), or any result at level `error`.
# `codeql.yml` runs it after `analyze`; run it locally on a CLI-made SARIF to preview the gate.
set -euo pipefail

IN=$1
OUT=$2
ACCEPTED="$(dirname "$0")/accepted-findings.json"

jq --slurpfile acc "$ACCEPTED" '
  ($acc[0].accepted | map({key: "\(.rule)\u0000\(.path)", value: true}) | from_entries) as $ok
  | .runs |= map(
      .results |= map(select(
        ($ok["\(.ruleId)\u0000\(.locations[0].physicalLocation.artifactLocation.uri)"] // false) | not
      ))
    )
' "$IN" > "$OUT"

# One line per blocking finding. Rules live on the driver or on an extension (query pack).
# A result without its own `level` takes its rule's default.
BLOCKING=$(jq -r '
  .runs[] as $run
  | [$run.tool.driver.rules[]?, $run.tool.extensions[]?.rules[]?] as $rules
  | ($rules | map({key: .id, value: ((.properties["security-severity"] // "0") | tonumber)}) | from_entries) as $sev
  | ($rules | map({key: .id, value: (.defaultConfiguration.level // "warning")}) | from_entries) as $lvl
  | $run.results[]
  | select(($sev[.ruleId] // 0) >= 7.0 or (.level // $lvl[.ruleId]) == "error")
  | "\(.ruleId) (severity \($sev[.ruleId] // "n/a")) at \(.locations[0].physicalLocation.artifactLocation.uri):\(.locations[0].physicalLocation.region.startLine): \(.message.text | split("\n")[0])"
' "$OUT")

KEPT=$(jq '[.runs[].results[]] | length' "$OUT")
DROPPED=$(( $(jq '[.runs[].results[]] | length' "$IN") - KEPT ))
echo "CodeQL: $KEPT finding(s) after dropping $DROPPED accepted one(s)."

if [ -n "$BLOCKING" ]; then
  echo "Blocking findings (high or critical):"
  while IFS= read -r line; do
    echo "::error::$line"
  done <<< "$BLOCKING"
  echo "Fix each one, or, for a false positive, add it to .github/codeql/accepted-findings.json with its reason."
  exit 1
fi
