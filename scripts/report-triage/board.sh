#!/usr/bin/env bash
# The triage agent's only way to touch the reports board: a curl wrapper over a fixed set of GitHub API calls on
# getcmdr/cmdr-reports. It exists because `gh` (Go) can't verify TLS inside the macOS sandbox, and it enforces in code what
# the prompt only asks for, since card text is untrusted and the prompt is the layer an injection can argue with.
#
# Usage:
#   ./board.sh list                       open cards labeled `unprocessed`, oldest first
#   ./board.sh view <n>                   one card: title, labels, body, and every comment
#   ./board.sh search <query>             issue search in this repo (open and closed), for duplicates
#   ./board.sh comment <n>                post a comment, body read from stdin
#   ./board.sh edit <n> [--title "..."] [--add-label X]... [--remove-label unprocessed]
#
# Guards (each refusal exits non-zero with a message on stderr):
#   - `comment` and `edit` first read the card and refuse unless it's an open issue carrying `unprocessed`.
#   - `comment` prepends a hidden marker and refuses when a comment already starts with it: one triage comment per card.
#   - Labels come from fixed allowlists; `--remove-label` only takes `unprocessed`, and goes last. No `severity:*` or
#     `size:*`: a card's rating is the last line of its triage comment, which the API server's cron copies onto the
#     board, so this script holds no board token at all.
#   - Titles are one line, 1–200 chars. Card numbers are positive integers. Search can't add its own scope qualifiers.
#   - Host api.github.com, repo getcmdr/cmdr-reports, and the HTTP calls below are hardcoded; nothing else is reachable.
#
# Reads GH_TOKEN from the environment (run.sh hands it a PAT scoped to this repo). It reaches curl through a config on
# a pipe, so it never shows in argv, and the script never prints it. Written for macOS's bash 3.2.
set -euo pipefail

readonly repo="getcmdr/cmdr-reports"
readonly api="https://api.github.com"
# The API server's rating job only reads comments that start with this (apps/api-server/src/report-ratings.ts).
readonly marker="<!-- cmdr:triage -->"
readonly addable_labels=" bug enhancement duplicate "
readonly removable_labels=" unprocessed "
readonly max_title=200
readonly max_comment=20000
readonly max_query=200

die() {
  echo "board.sh: $*" >&2
  exit 2
}

usage() {
  sed -n '/^# Usage:/,/^#$/p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

[[ -n "${GH_TOKEN:-}" ]] || die "GH_TOKEN isn't set"

check_number() {
  [[ "$1" =~ ^[1-9][0-9]{0,6}$ ]] || die "refusing: '$1' isn't a card number (a positive integer)"
}

# One GitHub API call. $1 is the method, $2 the path under the repo or a fixed top-level path; a request body, if any,
# comes on stdin. Only the methods this script uses get through.
gh_api() {
  local method="$1" path="$2"
  shift 2
  case "$method" in GET | POST | PATCH | DELETE) ;; *) die "internal: method $method not allowed" ;; esac
  curl --fail --silent --show-error --max-time 30 \
    --proto =https \
    --config <(printf 'header = "Authorization: Bearer %s"\n' "$GH_TOKEN") \
    --header "Accept: application/vnd.github+json" \
    --header "X-GitHub-Api-Version: 2022-11-28" \
    --request "$method" \
    "$@" \
    "$api$path"
}

# GET every page of a list endpoint and merge them into one JSON array. $1 is the path, already carrying a `?`.
gh_get_all() {
  local path="$1" page=1 chunk all="[]"
  while :; do
    chunk="$(gh_api GET "$path&per_page=100&page=$page")"
    all="$(jq -s '.[0] + .[1]' <(printf '%s' "$all") <(printf '%s' "$chunk"))"
    [[ "$(jq 'length' <<<"$chunk")" -lt 100 ]] && break
    page=$((page + 1))
    [[ $page -le 20 ]] || die "refusing: more than 2,000 items, something's off"
  done
  printf '%s' "$all"
}

# The guard every write starts with: the card must be an open issue (not a PR) carrying `unprocessed`.
require_unprocessed() {
  local n="$1" card
  card="$(gh_api GET "/repos/$repo/issues/$n")"
  jq -e '.pull_request == null' >/dev/null <<<"$card" || die "refusing: #$n is a pull request, not a card"
  [[ "$(jq -r '.state' <<<"$card")" == "open" ]] || die "refusing: #$n is closed; only open cards can be changed"
  jq -e '[.labels[].name] | index("unprocessed") != null' >/dev/null <<<"$card" ||
    die "refusing: #$n doesn't carry 'unprocessed'; only unprocessed cards can be changed"
}

cmd_list() {
  [[ $# -eq 0 ]] || usage
  gh_get_all "/repos/$repo/issues?state=open&labels=unprocessed&sort=created&direction=asc" |
    jq -r '.[] | select(.pull_request == null)
      | "#\(.number)\t\(.created_at)\t[\([.labels[].name] | join(", "))]\t\(.title)"'
}

cmd_view() {
  [[ $# -eq 1 ]] || usage
  check_number "$1"
  local card comments
  card="$(gh_api GET "/repos/$repo/issues/$1")"
  comments="$(gh_get_all "/repos/$repo/issues/$1/comments?")"
  jq -r '"#\(.number) (\(.state)): \(.title)", "Labels: \([.labels[].name] | join(", "))",
    "Created: \(.created_at)", "", "--- body ---", (.body // "")' <<<"$card"
  jq -r '.[] | "", "--- comment by \(.user.login) at \(.created_at) ---", (.body // "")' <<<"$comments"
}

cmd_search() {
  [[ $# -ge 1 ]] || usage
  local query="$*"
  [[ ${#query} -le $max_query ]] || die "refusing: the query is longer than $max_query chars"
  # GitHub ORs extra `repo:` qualifiers into the scope, so a query could widen the search past this repo.
  shopt -s nocasematch
  [[ ! "$query" =~ (^|[^[:alnum:]_-])(repo|org|user|owner|is|type): ]] ||
    die "refusing: the query can't carry its own repo/org/user/owner/is/type qualifiers"
  shopt -u nocasematch
  local q
  q="$(jq -rn --arg q "repo:$repo is:issue $query" '$q | @uri')"
  gh_api GET "/search/issues?q=$q&per_page=50" |
    jq -r '.items[] | "#\(.number)\t\(.state)\t[\([.labels[].name] | join(", "))]\t\(.title)"'
}

cmd_comment() {
  [[ $# -eq 1 ]] || usage
  check_number "$1"
  local n="$1" body
  body="$(cat)"
  [[ -n "${body//[[:space:]]/}" ]] || die "refusing: the comment body (stdin) is empty"
  [[ ${#body} -le $max_comment ]] || die "refusing: the comment is longer than $max_comment chars"
  require_unprocessed "$n"
  gh_get_all "/repos/$repo/issues/$n/comments?" |
    jq -e --arg m "$marker" 'any(.[]; (.body // "") | startswith($m)) | not' >/dev/null ||
    die "refusing: #$n already has a triage comment; it gets one, so carry on with the title and labels"
  jq -n --arg b "$marker"$'\n'"$body" '{body: $b}' |
    gh_api POST "/repos/$repo/issues/$n/comments" --header "Content-Type: application/json" --data-binary @- |
    jq -r '"Commented on #'"$n"': \(.html_url)"'
}

cmd_edit() {
  [[ $# -ge 1 ]] || usage
  check_number "$1"
  local n="$1" title="" has_title=0 add="" remove="" label
  shift
  [[ $# -ge 1 ]] || die "refusing: nothing to edit; pass --title, --add-label, or --remove-label"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --title)
        [[ $# -ge 2 ]] || die "refusing: --title needs a value"
        [[ $has_title -eq 0 ]] || die "refusing: --title given twice"
        title="$2" has_title=1
        [[ -n "${title//[[:space:]]/}" ]] || die "refusing: the title is empty"
        [[ ${#title} -le $max_title ]] || die "refusing: the title is longer than $max_title chars"
        [[ "$title" != *$'\n'* && "$title" != *$'\r'* ]] || die "refusing: the title must be one line"
        shift 2
        ;;
      --add-label | --remove-label)
        [[ $# -ge 2 ]] || die "refusing: $1 needs a value"
        label="$2"
        # One label per flag: without this, "bug enhancement" would match the space-padded allowlist below.
        [[ "$label" =~ ^[A-Za-z:]+$ ]] || die "refusing: '$label' isn't a single label name"
        if [[ "$1" == --add-label ]]; then
          [[ "$addable_labels" == *" $label "* ]] ||
            die "refusing: can't add '$label'; allowed:$addable_labels"
          add="$add $label"
        else
          [[ "$removable_labels" == *" $label "* ]] ||
            die "refusing: can't remove '$label'; only:$removable_labels"
          remove="$remove $label"
        fi
        shift 2
        ;;
      *) die "refusing: unknown option '$1'" ;;
    esac
  done
  require_unprocessed "$n"
  if [[ $has_title -eq 1 ]]; then
    jq -n --arg t "$title" '{title: $t}' |
      gh_api PATCH "/repos/$repo/issues/$n" --header "Content-Type: application/json" --data-binary @- >/dev/null
    echo "Renamed #$n"
  fi
  if [[ -n "$add" ]]; then
    # shellcheck disable=SC2086 # $add is a space-separated list of allowlisted labels, none containing spaces.
    jq -n '{labels: $ARGS.positional}' --args $add |
      gh_api POST "/repos/$repo/issues/$n/labels" --header "Content-Type: application/json" --data-binary @- >/dev/null
    echo "Added labels to #$n:$add"
  fi
  # Removing `unprocessed` goes last: it's what closes the card to every later write.
  for label in $remove; do
    gh_api DELETE "/repos/$repo/issues/$n/labels/$label" >/dev/null
    echo "Removed '$label' from #$n"
  done
}


[[ $# -ge 1 ]] || usage
sub="$1"
shift
case "$sub" in
  list) cmd_list "$@" ;;
  view) cmd_view "$@" ;;
  search) cmd_search "$@" ;;
  comment) cmd_comment "$@" ;;
  edit) cmd_edit "$@" ;;
  *) die "refusing: unknown subcommand '$sub'; use list, view, search, comment, or edit" ;;
esac
