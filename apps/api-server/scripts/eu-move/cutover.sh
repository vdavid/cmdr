#!/usr/bin/env bash
# The land-time cutover of the API server's D1 database and error-report bucket to their `eu`
# twins (vdavid/cmdr#805). The runbook that says when to run each phase is
# `docs/notes/eu-data-moves-2026-11.md` § "A2 cutover"; `worker.js` beside this is the tool it drives.
#
#   cutover.sh snapshot <file>   /validate answers for every license the server knows (no device id,
#                                so nothing is written), to compare before and after.
#   cutover.sh prepare <dir>     export the old database, import it into the empty EU one, save the
#                                marks, bump the EU sequences. Refuses unless the branch head has a
#                                green `CI OK` (so `pnpm land` lands at once) and the EU database is
#                                empty.
#   cutover.sh finish <dir>      once production binds the EU database: wait for the old one to go
#                                quiet, reconcile the window's rows, compare, copy the window's R2
#                                objects, and compare the /validate answers with <dir>/before.json.
#
# Environment: EU_MOVE_CONFIG (the tool's wrangler config; default the one beside this),
# EU_DB_NAME (default cmdr-telemetry-eu), and REHEARSAL=1 (with a config and name pointing at a
# scratch database: skips the land and deploy gates and the wait for the old database to go quiet).
# Never deletes anything. Foreground only: it starts the tool itself and stops it on exit.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
server_dir=$(cd "$here/../.." && pwd)
config=${EU_MOVE_CONFIG:-$here/wrangler.jsonc}
eu_db_name=${EU_DB_NAME:-cmdr-telemetry-eu}
old_db_name=cmdr-telemetry
eu_db_id=936d4ece-7f66-45d9-be68-dc1135790bdd
api=https://api.getcmdr.com
account_id=6a4433bf11c3cf86feda057f76f47991
bump_by=1000000

export CLOUDFLARE_ACCOUNT_ID=$account_id
export CLOUDFLARE_API_TOKEN
CLOUDFLARE_API_TOKEN=${CLOUDFLARE_API_TOKEN:-$(secret CLOUDFLARE_API_TOKEN)}

say() { printf '\n== %s\n' "$*" >&2; }
die() {
  printf '\nSTOP: %s\n' "$*" >&2
  exit 1
}

wrangler() { (cd "$server_dir" && pnpm exec wrangler "$@"); }

tool_port=
tool_pid=
start_tool() {
  tool_port=$((20000 + RANDOM % 9000))
  local log
  log=$(mktemp)
  (cd "$server_dir" && exec pnpm exec wrangler dev --config "$config" --ip 127.0.0.1 --port "$tool_port" \
    --show-interactive-dev-session=false) >"$log" 2>&1 &
  tool_pid=$!
  trap 'kill "$tool_pid" 2>/dev/null || true; wait "$tool_pid" 2>/dev/null || true' EXIT
  for _ in $(seq 1 120); do
    if curl -s -o /dev/null "http://127.0.0.1:$tool_port/"; then return; fi
    sleep 1
  done
  cat "$log" >&2
  die "the tool didn't start"
}

# GET one tool route; fails on an `error` field.
tool() {
  local out
  out=$(curl -sS --max-time 900 "http://127.0.0.1:$tool_port$1")
  if jq -e 'type == "object" and has("error")' >/dev/null <<<"$out"; then die "tool $1: $(jq -r .error <<<"$out")"; fi
  printf '%s' "$out"
}

snapshot() {
  local out=$1 licenses
  mkdir -p "$(dirname "$out")"
  licenses=$(curl -sS --fail "$api/admin/licenses" -H "Authorization: Bearer $(secret CMDR_ADMIN_API_TOKEN)")
  {
    jq -r '.licenses[].transactionId' <<<"$licenses"
    echo manual-ZZZZZZZZZZZZ # unknown manual id
    echo txn_00000000000000000000000000 # unknown Paddle id
  } | while read -r id; do
    local body code
    body=$(curl -sS -w '\n%{http_code}' -X POST "$api/validate" -H 'Content-Type: application/json' \
      -d "$(jq -cn --arg id "$id" '{transactionId: $id}')")
    code=${body##*$'\n'}
    jq -c --arg id "$id" --arg code "$code" '{id: $id, code: ($code | tonumber), body: .}' <<<"${body%$'\n'*}"
  done | jq -s 'sort_by(.id)' >"$out"
  say "snapshot: $(jq length "$out") answers in $out"
  jq -r '.[] | "\(.id[0:12])…  \(.code)  \(.body.status // .body.error // "?")"' "$out" >&2
}

land_gate() {
  [[ ${REHEARSAL:-} == 1 ]] && return
  local sha conclusion
  sha=$(git -C "$server_dir" rev-parse HEAD)
  [[ $(git -C "$server_dir" ls-remote origin "refs/heads/$(git -C "$server_dir" branch --show-current)" | cut -f1) == "$sha" ]] ||
    die "origin doesn't have HEAD ($sha) yet: push it with the land label first"
  conclusion=$(gh api "repos/vdavid/cmdr/commits/$sha/check-runs?check_name=CI%20OK" \
    --jq '.check_runs | sort_by(.id) | last | .conclusion // "none"')
  [[ $conclusion == success ]] ||
    die "HEAD $sha has no green \`CI OK\` (latest: $conclusion). Add the land label and wait for a land run."
}

deploy_gate() {
  [[ ${REHEARSAL:-} == 1 ]] && return
  local bound
  bound=$(curl -sS "https://api.cloudflare.com/client/v4/accounts/$account_id/workers/scripts/cmdr-license-server/settings" \
    -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" | jq -r '.result.bindings[] | select(.name == "TELEMETRY_DB") | .id')
  [[ $bound == "$eu_db_id" ]] || die "production still binds TELEMETRY_DB to $bound, not the EU database: wait for the deploy"
}

prepare() {
  local dir=$1
  mkdir -p "$dir"
  land_gate
  [[ -s $dir/before.json ]] || die "take the snapshot first: $0 snapshot $dir/before.json"
  [[ ! -e $dir/marks.json ]] || die "$dir/marks.json exists: this phase ran already"

  say "checking $eu_db_name is empty"
  local tables
  tables=$(wrangler d1 execute "$eu_db_name" --remote --json \
    --command "SELECT count(*) AS n FROM sqlite_master WHERE type = 'table' AND name NOT LIKE '_cf_%'" |
    jq '.[0].results[0].n')
  [[ $tables == 0 ]] || die "$eu_db_name already has $tables tables"

  say "exporting $old_db_name (it answers nothing for about 10 seconds)"
  local attempt
  for attempt in 1 2 3; do
    if wrangler d1 export "$old_db_name" --remote --output "$dir/prod.sql"; then break; fi
    [[ $attempt == 3 ]] && die "the export failed three times"
    sleep 5
  done

  say "importing into $eu_db_name"
  wrangler d1 execute "$eu_db_name" --remote -y --file "$dir/prod.sql" >"$dir/import.log" 2>&1 ||
    { cat "$dir/import.log" >&2; die "the import failed: $eu_db_name may be half-filled, see the runbook"; }

  start_tool
  say "checking the import"
  tool '/d1/stats?db=eu' >"$dir/stats-eu-import.json"
  tool '/d1/stats?db=old' >"$dir/stats-old-import.json"
  local migrations_eu migrations_old
  migrations_eu=$(jq '.tables.d1_migrations.rows' "$dir/stats-eu-import.json")
  migrations_old=$(jq '.tables.d1_migrations.rows' "$dir/stats-old-import.json")
  [[ $migrations_eu == "$migrations_old" && $migrations_eu -gt 0 ]] ||
    die "migrations: EU $migrations_eu, old $migrations_old"

  tool '/d1/marks?db=eu' >"$dir/marks.json"
  say "marks (the max ids at the export):"
  jq -c . "$dir/marks.json" >&2

  tool "/d1/bump?db=eu&by=$bump_by" >"$dir/bump.json"
  jq -e '.ok' "$dir/bump.json" >/dev/null || die "the bump left sequences short: $(jq -c .short "$dir/bump.json")"
  rm -f "$dir/prod.sql"
  say "prepared. Now: pnpm land (it lands at once), then after the deploy: $0 finish $dir"
}

finish() {
  local dir=$1
  [[ -s $dir/marks.json ]] || die "no $dir/marks.json: run prepare first"
  deploy_gate
  start_tool

  say "waiting for the old database to go quiet"
  local a b
  a=$(tool '/d1/stats?db=old' | jq -c '[.tables[] | .maxId]')
  for _ in $(seq 1 15); do
    [[ ${REHEARSAL:-} == 1 ]] && { b=$a; break; }
    sleep 60
    b=$(tool '/d1/stats?db=old' | jq -c '[.tables[] | .maxId]')
    [[ $a == "$b" ]] && break
    a=$b
  done
  [[ $a == "$b" ]] || die "the old database is still being written after 15 minutes: is production on the EU one?"
  tool '/d1/stats?db=old' >"$dir/stats-old-final.json"

  local marks
  marks=$(jq -sRr @uri "$dir/marks.json")
  say "reconcile, dry"
  tool "/d1/reconcile?from=old&to=eu&marks=$marks" >"$dir/reconcile-dry.json"
  jq -c '{appended, keyedCopied, stamped}' "$dir/reconcile-dry.json" >&2
  say "reconcile, applied"
  tool "/d1/reconcile?from=old&to=eu&marks=$marks&apply=1" >"$dir/reconcile.json"
  jq -c '{appended, ignored, keyedCopied, stamped, differing: (.differing | map_values(length))}' "$dir/reconcile.json" >&2

  say "R2: copying objects written since the first copy"
  # A key that expires between the listing and the copy stays "missing", so the loop is bounded.
  local remaining=1 round=0
  while [[ $remaining != 0 && $round -lt 10 ]]; do
    remaining=$(tool '/r2/copy?from=old&to=eu&limit=500' | jq '.remaining')
    round=$((round + 1))
  done
  tool '/r2/diff?from=old&to=eu' >"$dir/r2-diff.json"
  jq -c '{fromCount, toCount, missing: (.missing | length), differing: [.differing[].key]}' "$dir/r2-diff.json" >&2

  say "comparing /validate answers"
  snapshot "$dir/after.json"
  if diff <(jq -S . "$dir/before.json") <(jq -S . "$dir/after.json") >&2; then
    say "/validate: identical"
  else
    die "/validate answers changed (diff above): consider the rollback"
  fi

  local left
  left=$(jq '[.ignored[]?] + [.differing[] | length] | add // 0' "$dir/reconcile.json")
  if [[ $left != 0 ]]; then
    say "done, with $left row(s) for a human: see .ignored and .differing in $dir/reconcile.json"
  else
    say "done: nothing left to settle"
  fi
}

case ${1:-} in
  snapshot) snapshot "${2:?file}" ;;
  prepare) prepare "${2:?dir}" ;;
  finish) finish "${2:?dir}" ;;
  *) die "usage: $0 snapshot <file> | prepare <dir> | finish <dir>" ;;
esac
