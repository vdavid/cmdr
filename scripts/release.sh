#!/bin/bash
# Makes a release from a tested commit. On top of `origin/main` (call it X) it commits
# `chore(release): vX.Y.Z` (call it R) on a short branch, `release/vX.Y.Z`, and signs the tag vX.Y.Z
# on R itself. Then, side by side: the tag push builds (in `local` mode only into a private draft)
# and one full (`run_all`) CI run plus CodeQL test R. Nothing ships until that run is green:
# `release-finish.sh` waits for it before it signs, and the pipeline's `ci-gate` job checks it again
# before `publish`. `main` keeps moving meanwhile; once the release is out, R goes back into `main`
# through `pnpm land -merge`, never rebased, so the tag stays on `main`'s history.
#
# Re-running it with the same version resumes wherever it stopped, and once the release is published
# it merges R back into `main`. The whole flow: `docs/guides/releasing.md` § The release procedure.
#
# Usage:
#   ./scripts/release.sh <version>          make the release, resume it, or merge it back once published
#   ./scripts/release.sh -abort <version>   drop an unpublished release: its runs, draft, tag, and branch
#   ./scripts/release.sh -test <version>    pipeline test: <version> like 0.55.1-pipelinetest.1, built
#                                           from this branch's HEAD into a private draft, never merged
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

MODE=release
case "${1:-}" in
  -abort) MODE=abort && shift ;;
  -test) MODE=test && shift ;;
esac
VERSION="${1:-}"
if [[ -z "$VERSION" || $# -ne 1 ]]; then
  sed -n '/^# Usage:/,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'
  exit 1
fi
if [[ "$MODE" == test ]]; then
  if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+-pipelinetest\.[0-9]+$ ]]; then
    echo "Error: a pipeline test's version looks like 0.55.1-pipelinetest.1."
    exit 1
  fi
elif ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] && ! [[ "$MODE" == abort && "$VERSION" =~ -pipelinetest\.[0-9]+$ ]]; then
  echo "Error: Version must be in format X.Y.Z (e.g., 0.2.1)"
  exit 1
fi

TAG="v$VERSION"
RELEASE_BRANCH="release/$TAG"
RELEASE_SUBJECT="chore(release): $TAG"
# Who signs the update archives (docs/guides/releasing.md § Who signs the update archives). Unset is `ci`.
SIGNING=$(gh variable get RELEASE_UPDATE_SIGNING 2>/dev/null || echo ci)

# The draft flag of each release on the tag: `true`, `false`, or nothing. The full list is the one
# place a draft shows with its tag.
release_drafts() {
  gh api --paginate "repos/{owner}/{repo}/releases?per_page=100" --jq ".[] | select(.tag_name == \"$TAG\") | .draft"
}

is_published() {
  release_drafts | grep -qx false
}

# Everything up to the release commit and its tag. Ends back on the branch it started from, with R
# on $RELEASE_BRANCH and the signed tag on R.
prepare_release() {
  local start_branch
  start_branch=$(git branch --show-current)
  if [[ "$MODE" == release && "$start_branch" != main ]]; then
    echo "Error: run this on main (in the main clone), which it branches $RELEASE_BRANCH from."
    exit 1
  fi
  if [[ "$MODE" == test && "$SIGNING" != local ]]; then
    echo "Error: RELEASE_UPDATE_SIGNING is '$SIGNING'. A test tag must only build into a draft, and in 'ci' mode it would PUBLISH."
    exit 1
  fi

  # Check for uncommitted changes. The release-prep edits are allowed through and get included in
  # the release commit: the changelog, the roadmap (content in roadmap.ts, layout in roadmap.astro),
  # the feature-status source of truth (plus the features page, which needs a card for every new
  # feature id or it fails the build), and the security advisory list (its new entry names this
  # version, which the `desktop-security-advisories` check only accepts once the bump below lands).
  # See docs/guides/releasing.md § The release procedure for the steps that produce them.
  local exclude=(
    ':!CHANGELOG.md'
    ':!feature-status.json'
    ':!apps/website/src/pages/features.astro'
    ':!apps/website/src/lib/roadmap.ts'
    ':!apps/website/src/pages/roadmap.astro'
    ':!apps/desktop/src-tauri/resources/security-advisories.json'
  )
  if ! git diff --quiet -- "${exclude[@]}" || ! git diff --staged --quiet -- "${exclude[@]}"; then
    echo "Error: Working tree has uncommitted changes (other than the changelog, roadmap, feature status and its page, and security advisory list). Commit or stash them first."
    exit 1
  fi

  # Detach stale Cmdr* DMG mounts. A leftover /Volumes/Cmdr (typically from inspecting an old DMG in
  # Finder) makes `bundle_dmg.sh` fail fast on a self-hosted runner on this same Mac, because the
  # volume name is already taken. The release workflow has the same guard before its bundle step.
  while IFS= read -r vol; do
    if [[ -n "$vol" ]]; then
      echo "Detaching stale mount: $vol"
      hdiutil detach "$vol" -force >/dev/null 2>&1 || true
    fi
  done < <(mount | awk -F' on ' '/\/Volumes\/Cmdr/ { sub(/ \(.*$/, "", $2); print $2 }')

  if [[ "$MODE" == release ]]; then
    # Stage allowed files so the pull below carries them along
    git add CHANGELOG.md feature-status.json apps/website/src/pages/features.astro apps/website/src/lib/roadmap.ts apps/website/src/pages/roadmap.astro apps/desktop/src-tauri/resources/security-advisories.json 2>/dev/null || true

    # The release commit goes on top of origin/main's HEAD, X. Local `main` mirrors `origin/main`,
    # so this only ever fast-forwards; a local commit on `main` makes it refuse (it belongs on a
    # branch, landed through `pnpm land`). --autostash carries the staged release-prep edits along.
    git pull --ff-only --autostash origin main
  fi

  # Check CHANGELOG.md has an [Unreleased] section with content. A pipeline test ships no notes.
  if [[ "$MODE" == release ]]; then
    if ! grep -q '## \[Unreleased\]' CHANGELOG.md; then
      echo "Error: CHANGELOG.md has no [Unreleased] section."
      echo "Add a '## [Unreleased]' heading with release notes before the first versioned section."
      exit 1
    fi
    local unreleased
    unreleased=$(sed -n '/## \[Unreleased\]/,/## \[/p' CHANGELOG.md | sed '1d;$d' | grep -v '^$' || true)
    if [[ -z "$unreleased" ]]; then
      echo "Error: The [Unreleased] section in CHANGELOG.md is empty."
      echo "Add release notes under it before releasing!"
      exit 1
    fi
  fi

  # Every changelog ref must be a commit on `main` (reachable from HEAD), or the website links it
  # to nothing. `--fresh`: its cache keys on CHANGELOG.md alone, and reachability is about history.
  pnpm check changelog-links -m --fresh

  echo "Releasing version $VERSION on top of $(git rev-parse --short=9 HEAD)..."

  # Update version in package.json
  (cd apps/desktop && npm pkg set version="$VERSION")

  # Record the settings defaults THIS release ships with, so the analytics dashboard can tell
  # "the user is on the default" from "the setting didn't exist yet" for installs on it. The
  # manifest only gains an entry when a default actually moved, but every run stamps
  # `promotedThrough`, which is how the check tells a release that changed nothing from one nobody
  # recorded. Skipping it fails `analytics-settings-defaults` on the next check run.
  if [[ "$MODE" == release ]]; then
    (cd apps/desktop && node scripts/gen-analytics-defaults.ts --promote "$VERSION")
  else
    # A pipeline test records no defaults (`--promote` takes X.Y.Z only); the stamp alone keeps
    # `analytics-settings-defaults` green on the test commit's CI run.
    local manifest=apps/analytics-dashboard/src/lib/server/settings-defaults.gen.json
    jq --arg v "$VERSION" '.promotedThrough = $v' "$manifest" > "$manifest.tmp" && mv "$manifest.tmp" "$manifest"
  fi

  # Update version in tauri.conf.json
  (cd apps/desktop/src-tauri && jq ".version = \"$VERSION\"" tauri.conf.json > tauri.conf.json.tmp && mv tauri.conf.json.tmp tauri.conf.json)

  # Update version in Cargo.toml and sync Cargo.lock
  sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" apps/desktop/src-tauri/Cargo.toml
  (cd apps/desktop/src-tauri && cargo update --workspace --quiet)

  # Update CHANGELOG.md: replace [Unreleased] with the versioned heading
  local today
  today=$(date +%Y-%m-%d)
  sed -i '' "s/## \[Unreleased\]/## [$VERSION] - $today/" CHANGELOG.md

  # Roll the BSL Change Date forward so THIS release converts to AGPL three years after it
  # ships. The Change Date is a static field, not a rolling window: left alone, every version
  # ever shipped converts on one shared date, and the protection window shrinks with every
  # release (a build shipped in December 2028 against a 2029-01-10 date would go AGPL a month
  # later). BSL takes whichever of the Change Date and the version's fourth anniversary comes
  # FIRST, so the anniversary can only ever pull conversion earlier; three years leaves a year
  # of headroom under that four-year cap.
  local change_date
  change_date=$(date -v+3y +%Y-%m-%d)
  sed -i '' "s/^Change Date:.*/Change Date:          $change_date/" LICENSE

  # Refresh the website's visual baselines against the finalized release copy, so a release can't
  # ship a stale one and turn the release's CI run red. The baselines are region-scoped and no
  # longer cover /features, so release-prep copy shouldn't move any of them; this stays as a cheap
  # guard, and the `git add -u` below stages whatever actually moved. Requires Docker (the Linux
  # baselines can't be rendered on macOS); a missing/stopped Docker aborts here, before committing.
  apps/website/scripts/update-visual-baselines.sh

  # Run oxfmt across the repo so CHANGELOG / package.json / tauri.conf.json drift from
  # manual edits + the sed/`npm pkg set` mutations above doesn't fail CI on the release commit.
  # This also reformats any unrelated files that drifted (for example, a `.claude/commands/*.md`
  # touched in the same uncommitted batch the user is releasing).
  # `-m`: the release runs in the main clone by design, so opt past the worktree-only
  # guard (the `--ci` gate below opts past it via --ci instead).
  pnpm check oxfmt -m

  # Stage the files the script itself just bumped.
  git add \
    CHANGELOG.md \
    LICENSE \
    feature-status.json \
    apps/website/src/pages/features.astro \
    apps/website/src/lib/roadmap.ts \
    apps/website/src/pages/roadmap.astro \
    apps/desktop/src-tauri/resources/security-advisories.json \
    apps/desktop/package.json \
    apps/desktop/src-tauri/tauri.conf.json \
    apps/desktop/src-tauri/Cargo.toml \
    Cargo.lock

  # Pick up anything the steps above modified on top of those. `git add -u` only touches
  # tracked files that are already modified, and the pre-flight at the top of this script
  # guaranteed the working tree was clean before we started, so the only modifications that
  # exist now are the version bumps above, the refreshed visual baselines, plus oxfmt's
  # auto-fixes. This keeps the release commit in sync with what oxfmt --ci will see in CI.
  git add -u

  # Belt-and-braces: confirm the staged tree passes oxfmt in CI mode (no auto-fix). If this
  # fails, the release commit would carry formatting drift that CI rejects, so abort
  # instead of committing.
  pnpm check oxfmt --ci

  # Release gate: a stale translation must NOT ship. The i18n-stale check is warn-only in
  # normal `pnpm check` (a maintenance signal, not a daily-dev build breaker), but at release
  # we escalate it to a build-failing ERROR via CMDR_I18N_STALE_STRICT. With `set -e`, a stale
  # finding aborts the release before we tag, so the fix lands first. (English-only today, so
  # this is a clean no-op until a real locale exists.) See docs/guides/releasing.md.
  CMDR_I18N_STALE_STRICT=1 pnpm check i18n-stale -m

  # The staged release commit goes on its own branch; local `main` stays a mirror of origin's.
  git switch -c "$RELEASE_BRANCH"
  git commit -m "$RELEASE_SUBJECT"
  tag_release_commit
  git switch "$start_branch"
}

# A signed annotated tag on R proves the release trigger came from the trusted key whose public half
# lives in `.github/release-signers`; the workflow verifies it before building anything. `-s`
# follows the configured signing format and key; a missing key or a declined passphrase aborts
# here rather than leaving an unsigned release tag behind.
tag_release_commit() {
  git tag -s -m "Cmdr $TAG" "$TAG" "$RELEASE_BRANCH"
}

# The newest run of a workflow on R, if any: the dispatched CI or CodeQL run.
dispatched_run() {
  gh api "repos/{owner}/{repo}/actions/workflows/$1/runs?head_sha=$R&event=workflow_dispatch&per_page=1" \
    --jq '.workflow_runs[0].id // empty'
}

# Starts a workflow on the release branch unless R already has a dispatched run of it, and waits
# for the run to show up, so `release-finish.sh` finds it.
dispatch_on_branch() {
  local workflow="$1"
  shift
  if [[ -n "$(dispatched_run "$workflow")" ]]; then
    return
  fi
  echo "Starting $workflow on $RELEASE_BRANCH..."
  gh workflow run "$workflow" --ref "$RELEASE_BRANCH" "$@"
  for _ in $(seq 1 12); do
    sleep 5
    [[ -n "$(dispatched_run "$workflow")" ]] && return
  done
  echo "Error: started $workflow, but no run showed up on ${R:0:9} within a minute. Check the Actions tab, then re-run this."
  exit 1
}

# Pushes R's branch and starts its full CI run and CodeQL, then pushes the tag: in `local` mode
# right away (it builds into a private draft beside CI), in `ci` mode only once CI is green, since
# that tag push publishes on its own.
push_and_test() {
  local remote_branch remote_tag
  remote_branch=$(git ls-remote --heads origin "refs/heads/$RELEASE_BRANCH" | cut -f1)
  if [[ -z "$remote_branch" ]]; then
    echo "Pushing $RELEASE_BRANCH..."
    # `--no-verify`: the pre-push hook may commit a formatting fix, and the tag is on R. R passed
    # `oxfmt --ci` above, and its CI run checks the rest.
    git push --quiet --no-verify origin "$R:refs/heads/$RELEASE_BRANCH"
  elif [[ "$remote_branch" != "$R" ]]; then
    echo "Error: origin's $RELEASE_BRANCH is at ${remote_branch:0:9}, not the release commit ${R:0:9}. A leftover from an aborted release? Delete it (git push origin --delete $RELEASE_BRANCH), then re-run."
    exit 1
  fi

  dispatch_on_branch ci.yml -f run_all=true
  dispatch_on_branch codeql.yml

  remote_tag=$(git ls-remote --tags origin "refs/tags/$TAG" | cut -f1)
  if [[ -n "$remote_tag" ]]; then
    if [[ "$remote_tag" != "$(git rev-parse "$TAG")" ]]; then
      echo "Error: origin has a different $TAG. Find out where it came from before going on."
      exit 1
    fi
    return
  fi
  if [[ "$SIGNING" != local ]]; then
    # `ci` mode: the tag push's own run publishes, so the gate comes first.
    ./scripts/release-ci-gate.sh "$VERSION"
  fi
  echo "Pushing $TAG..."
  git push --quiet origin "refs/tags/$TAG"
}

# Once the release is out, R goes into `main`: `land -merge` never rebases it, so the tag stays on
# `main`'s history. A fast-forward when `main` hasn't moved since X, else a merge commit GitHub makes.
merge_back() {
  git fetch --quiet origin main
  if git merge-base --is-ancestor "$R" origin/main; then
    echo "$TAG is on main already."
  else
    if [[ -z "$(git branch --list "$RELEASE_BRANCH")" ]]; then
      git branch "$RELEASE_BRANCH" "$R"
    fi
    local back
    back=$(git branch --show-current)
    git switch "$RELEASE_BRANCH"
    pnpm land -merge
    git switch "$back"
  fi
  git branch -D "$RELEASE_BRANCH" 2>/dev/null || true
  echo "$TAG is released and on main."
}

# Drops a release that never went public: stops its runs (so nothing uploads into a draft about to
# go, and no runner keeps working for it), deletes the build artifacts and the draft, then the tag
# and the branch, on origin and here. Nothing public moves in an unpublished release: `latest.json`
# and the Homebrew tap change only at publish.
abort_release() {
  if is_published; then
    echo "Error: $TAG is published, so it can't be aborted. Ship a fix in a new release."
    exit 1
  fi
  local id
  for id in $(gh run list -w release.yml -b "$TAG" -L 20 --json databaseId,status --jq '.[] | select(.status != "completed") | .databaseId') \
    $(gh run list -w ci.yml -b "$RELEASE_BRANCH" -L 20 --json databaseId,status --jq '.[] | select(.status != "completed") | .databaseId') \
    $(gh run list -w codeql.yml -b "$RELEASE_BRANCH" -L 20 --json databaseId,status --jq '.[] | select(.status != "completed") | .databaseId'); do
    echo "Stopping run $id..."
    gh run cancel "$id" >/dev/null || true
  done
  for id in $(gh run list -w release.yml -b "$TAG" -L 20 --json databaseId --jq '.[].databaseId'); do
    for artifact in $(gh api "repos/{owner}/{repo}/actions/runs/$id/artifacts" --jq '.artifacts[].id'); do
      gh api -X DELETE "repos/{owner}/{repo}/actions/artifacts/$artifact" && echo "Deleted artifact $artifact of run $id."
    done
  done
  for id in $(gh api --paginate "repos/{owner}/{repo}/releases?per_page=100" --jq ".[] | select(.tag_name == \"$TAG\" and .draft) | .id"); do
    gh api -X DELETE "repos/{owner}/{repo}/releases/$id" && echo "Deleted draft release $id."
  done
  if [[ -n "$(git ls-remote --tags origin "refs/tags/$TAG")" ]]; then
    git push --quiet origin ":refs/tags/$TAG" && echo "Deleted $TAG on origin."
  fi
  if [[ -n "$(git ls-remote --heads origin "refs/heads/$RELEASE_BRANCH")" ]]; then
    git push --quiet --no-verify origin --delete "$RELEASE_BRANCH" && echo "Deleted $RELEASE_BRANCH on origin."
  fi
  git tag -d "$TAG" 2>/dev/null || true
  if [[ "$(git branch --show-current)" == "$RELEASE_BRANCH" ]]; then
    git switch main
  fi
  git branch -D "$RELEASE_BRANCH" 2>/dev/null || true
  echo "$TAG is gone. Land the fix on main, then release again: ./scripts/release.sh $VERSION"
}

if [[ "$MODE" == abort ]]; then
  abort_release
  exit 0
fi

# Re-running after a stop picks up where it stopped: a tagged release commit goes on to its pushes,
# a release commit without a tag gets one, and a published release merges back into `main`.
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  R=$(git rev-parse "$TAG^{commit}")
  if [[ "$(git log -1 --format=%s "$R")" != "$RELEASE_SUBJECT" ]]; then
    echo "Error: the local tag $TAG isn't on a '$RELEASE_SUBJECT' commit. Release a new version instead."
    exit 1
  fi
  echo "Resuming $TAG: the release commit is ${R:0:9}."
elif [[ -n "$(git branch --list "$RELEASE_BRANCH")" && "$(git log -1 --format=%s "$RELEASE_BRANCH")" == "$RELEASE_SUBJECT" ]]; then
  echo "Resuming $TAG: tagging the release commit on $RELEASE_BRANCH."
  tag_release_commit
  R=$(git rev-parse "$TAG^{commit}")
else
  prepare_release
  R=$(git rev-parse "$TAG^{commit}")
fi

if [[ "$MODE" == release ]] && is_published; then
  merge_back
  exit 0
fi

push_and_test

REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
echo ""
echo "$TAG is pushed: the release commit ${R:0:9} on $RELEASE_BRANCH, with its full CI run and CodeQL going."
echo "  CI on it:   https://github.com/$REPO/actions/workflows/ci.yml?query=branch%3A${RELEASE_BRANCH//\//%2F}"
echo "  The build:  https://github.com/$REPO/actions/workflows/release.yml?query=branch%3A$TAG"
echo "  Watch CI alone (stops on the first red job): ./scripts/release-ci-gate.sh $VERSION"
echo "  Drop it (red CI, a broken build): ./scripts/release.sh -abort $VERSION"
if [[ "$MODE" == test ]]; then
  echo "Pipeline test: once both are green, ./scripts/release-finish.sh -dry-run -out <dir> $VERSION, then clean up with -abort."
elif [[ "$SIGNING" == local ]]; then
  # The tag push builds a draft; this laptop signs the update archives once CI and the build are
  # green, and has CI publish. docs/guides/releasing.md § Who signs the update archives.
  echo "Next: caffeinate -i ./scripts/release-finish.sh $VERSION (waits for CI and the build, signs, publishes)."
  echo "Once it's published: ./scripts/release.sh $VERSION merges the release commit back into main."
else
  echo "The tag push publishes once its build is green. Then: ./scripts/release.sh $VERSION merges the release commit back into main."
fi
