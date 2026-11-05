#!/bin/bash
# Prints the browser demo image the site serves: the one the release GitHub marks "Latest"
# published (or the release named as the first argument), as two lines for `$GITHUB_OUTPUT`:
#
#   version=v0.60.0
#   digest=<64 hex chars>
#
# The release pipeline's `demo` job builds the demo from the tagged sources, pushes it as
# `ghcr.io/vdavid/cmdr-demo:<tag>`, and attests it; a release can't publish without it. This
# resolves the tag to its digest and refuses an image the release pipeline didn't attest, so a
# moved tag can't put another build on the site. The website lane (to build and test /demo/) and
# the deploy (to pin the Dockerfile's `DEMO_IMAGE_DIGEST`) both read it, so CI tests exactly the
# demo the deploy ships. "Latest" is the same pointer the site's `latest.json` follows
# (`apps/website/scripts/fetch-live-manifest.ts`). `apps/website/DETAILS.md` § Browser demo.
#
# Needs `gh` (authenticated; GH_TOKEN in CI) and Docker with buildx. The image is public, so no
# registry login.
#
# Usage: ./scripts/released-demo.sh [tag]
set -euo pipefail

REPO="${GITHUB_REPOSITORY:-vdavid/cmdr}"
IMAGE="ghcr.io/vdavid/cmdr-demo"
SIGNER="$REPO/.github/workflows/release-pipeline.yml"

# Releases up to this one published no demo image (the pipeline started publishing after it).
# Theirs is the demo `main` deployed when the cutover landed, pinned here so a rollback to one
# of them still finds a demo. Content-keyed tag `5e2c415ff149d61f1274c695778aa9191a297840`,
# read 2026-11-06 with `docker buildx imagetools inspect`.
LAST_RELEASE_WITHOUT_DEMO="0.59.0"
PRE_CUTOVER_DIGEST="97d5491b139282e2a37d4b5bcaa8f98e8196f9418b361c99c8842784910ac3c1"

tag="${1:-}"
if [[ -z "$tag" ]]; then
    tag=$(gh api "repos/$REPO/releases/latest" --jq .tag_name)
fi
if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "The release tag is '$tag', not vX.Y.Z." >&2
    exit 1
fi

version="${tag#v}"
if [[ "$(printf '%s\n%s\n' "$version" "$LAST_RELEASE_WITHOUT_DEMO" | sort -V | tail -1)" == "$LAST_RELEASE_WITHOUT_DEMO" ]]; then
    echo "$tag predates release-published demos, so its demo is the cutover's ($PRE_CUTOVER_DIGEST)." >&2
    echo "version=$tag"
    echo "digest=$PRE_CUTOVER_DIGEST"
    exit 0
fi

if ! digest=$(docker buildx imagetools inspect "$IMAGE:$tag" --format '{{.Manifest.Digest}}'); then
    echo "$IMAGE:$tag doesn't exist, though every release after $LAST_RELEASE_WITHOUT_DEMO publishes one (the release pipeline's \`demo\` job)." >&2
    exit 1
fi
if [[ ! "$digest" =~ ^sha256:[0-9a-f]{64}$ ]]; then
    echo "Couldn't read the digest of $IMAGE:$tag (got '$digest')." >&2
    exit 1
fi

# Only the release pipeline signs these, so a tag someone moved by hand fails here.
if ! gh attestation verify "oci://$IMAGE@$digest" --repo "$REPO" --signer-workflow "$SIGNER" >/dev/null; then
    echo "$IMAGE:$tag ($digest) carries no provenance from $SIGNER; refusing it." >&2
    exit 1
fi

echo "$IMAGE:$tag is $digest, attested by the release pipeline." >&2
echo "version=$tag"
echo "digest=${digest#sha256:}"
