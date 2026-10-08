#!/bin/bash
# Exports the live-provider variables cmdr-s3's `live_` cells read
# (`crates/cmdr-s3/src/volume/live_support.rs`), for every account Cmdr tests
# against: R2, Hetzner, GCS, DigitalOcean Spaces, AWS, Backblaze B2, and
# Wasabi. Source it; don't run it:
#
#   source apps/desktop/test/s3-servers/live-env.sh
#
# Credentials come from David's sops store through the `secret` CLI and go to
# the environment only. ❗ Never echo them. Bucket names aren't secret, so the
# ones without a secret of their own are defaults here; any variable already
# set wins. Hetzner and Spaces have no default bucket: they bill while a bucket
# exists, so they run only when one is passed in.
#
# Each provider with a second bucket (`_BUCKET_2`, reached by the same key)
# runs the cross-bucket cells too.

export CMDR_S3_LIVE=1

# Cloudflare R2: the account ID comes from the API token, so it isn't written
# down here. The key is scoped to one bucket.
if [ -z "${CMDR_S3_LIVE_R2_ACCOUNT:-}" ]; then
    _cf_token="$(secret CLOUDFLARE_API_TOKEN)"
    CMDR_S3_LIVE_R2_ACCOUNT="$(curl -fsS -H "Authorization: Bearer $_cf_token" \
        https://api.cloudflare.com/client/v4/accounts | jq -r '.result[0].id')"
    unset _cf_token
fi
export CMDR_S3_LIVE_R2_ACCOUNT
export CMDR_S3_LIVE_R2_KEY_ID="${CMDR_S3_LIVE_R2_KEY_ID:-$(secret R2_S3_TEST_ACCESS_KEY_ID)}"
export CMDR_S3_LIVE_R2_SECRET="${CMDR_S3_LIVE_R2_SECRET:-$(secret R2_S3_TEST_SECRET_ACCESS_KEY)}"
export CMDR_S3_LIVE_R2_BUCKET="${CMDR_S3_LIVE_R2_BUCKET:-cmdr-s3-test}"

# Hetzner Object Storage, only when CMDR_S3_LIVE_HETZNER_BUCKET is set: Hetzner
# bills per hour while a bucket exists, so the test buckets are made for a run
# and deleted after (README § "Live providers"). HEL1 has been degraded; NBG1
# is the default. An unset bucket leaves every Hetzner variable unset, and the
# cells skip it.
if [ -n "${CMDR_S3_LIVE_HETZNER_BUCKET:-}" ]; then
    export CMDR_S3_LIVE_HETZNER_BUCKET
    export CMDR_S3_LIVE_HETZNER_LOCATION="${CMDR_S3_LIVE_HETZNER_LOCATION:-nbg1}"
    export CMDR_S3_LIVE_HETZNER_KEY_ID="${CMDR_S3_LIVE_HETZNER_KEY_ID:-$(secret HETZNER_S3_ACCESS_KEY_ID)}"
    export CMDR_S3_LIVE_HETZNER_SECRET="${CMDR_S3_LIVE_HETZNER_SECRET:-$(secret HETZNER_S3_SECRET_ACCESS_KEY)}"
    if [ -n "${CMDR_S3_LIVE_HETZNER_BUCKET_2:-}" ]; then export CMDR_S3_LIVE_HETZNER_BUCKET_2; fi
fi

# Google Cloud Storage, through the XML API with HMAC keys.
export CMDR_S3_LIVE_GCS_KEY_ID="${CMDR_S3_LIVE_GCS_KEY_ID:-$(secret GCS_S3_ACCESS_KEY_ID)}"
export CMDR_S3_LIVE_GCS_SECRET="${CMDR_S3_LIVE_GCS_SECRET:-$(secret GCS_S3_SECRET_ACCESS_KEY)}"
export CMDR_S3_LIVE_GCS_BUCKET="${CMDR_S3_LIVE_GCS_BUCKET:-$(secret GCS_S3_TEST_BUCKET)}"

# DigitalOcean Spaces, only when CMDR_S3_LIVE_SPACES_BUCKET is set: Spaces bills
# $5/month while any bucket exists, so the test bucket is made for a run and
# deleted after (README § "Live providers"). The key is bucket-scoped to the
# name in `secret DO_SPACES_TEST_BUCKET`, so recreate that one.
if [ -n "${CMDR_S3_LIVE_SPACES_BUCKET:-}" ]; then
    export CMDR_S3_LIVE_SPACES_BUCKET
    export CMDR_S3_LIVE_SPACES_REGION="${CMDR_S3_LIVE_SPACES_REGION:-$(secret DO_SPACES_REGION)}"
    export CMDR_S3_LIVE_SPACES_KEY_ID="${CMDR_S3_LIVE_SPACES_KEY_ID:-$(secret DO_SPACES_ACCESS_KEY_ID)}"
    export CMDR_S3_LIVE_SPACES_SECRET="${CMDR_S3_LIVE_SPACES_SECRET:-$(secret DO_SPACES_SECRET_ACCESS_KEY)}"
fi

# AWS, as the IAM user `claude-agent`. Two buckets in eu-north-1, plus a third
# in us-west-2 for the account root's per-bucket region routing.
export CMDR_S3_LIVE_AWS_REGION="${CMDR_S3_LIVE_AWS_REGION:-eu-north-1}"
export CMDR_S3_LIVE_AWS_KEY_ID="${CMDR_S3_LIVE_AWS_KEY_ID:-$(secret AWS_CMDR_ACCESS_KEY)}"
export CMDR_S3_LIVE_AWS_SECRET="${CMDR_S3_LIVE_AWS_SECRET:-$(secret AWS_CMDR_SECRET_ACCESS_KEY)}"
export CMDR_S3_LIVE_AWS_BUCKET="${CMDR_S3_LIVE_AWS_BUCKET:-cmdr-s3-test-58fb74}"
export CMDR_S3_LIVE_AWS_BUCKET_2="${CMDR_S3_LIVE_AWS_BUCKET_2:-cmdr-s3-test-58fb74-2}"
export CMDR_S3_LIVE_AWS_FAR_BUCKET="${CMDR_S3_LIVE_AWS_FAR_BUCKET:-cmdr-s3-test-58fb74-usw2}"
export CMDR_S3_LIVE_AWS_FAR_REGION="${CMDR_S3_LIVE_AWS_FAR_REGION:-us-west-2}"

# Backblaze B2, through its S3 API. The master key doesn't work there, so this
# is an application key reaching the two test buckets; the scoped one reaches
# `_BUCKET_2` only, for the refusal cells.
export CMDR_S3_LIVE_B2_REGION="${CMDR_S3_LIVE_B2_REGION:-eu-central-003}"
export CMDR_S3_LIVE_B2_KEY_ID="${CMDR_S3_LIVE_B2_KEY_ID:-$(secret B2_S3_TEST_KEY_ID)}"
export CMDR_S3_LIVE_B2_SECRET="${CMDR_S3_LIVE_B2_SECRET:-$(secret B2_S3_TEST_SECRET_ACCESS_KEY)}"
export CMDR_S3_LIVE_B2_BUCKET="${CMDR_S3_LIVE_B2_BUCKET:-cmdr-s3-test-58fb74}"
export CMDR_S3_LIVE_B2_BUCKET_2="${CMDR_S3_LIVE_B2_BUCKET_2:-cmdr-s3-test-58fb74-2}"
export CMDR_S3_LIVE_B2_SCOPED_KEY_ID="${CMDR_S3_LIVE_B2_SCOPED_KEY_ID:-$(secret B2_S3_SCOPED_KEY_ID)}"
export CMDR_S3_LIVE_B2_SCOPED_SECRET="${CMDR_S3_LIVE_B2_SCOPED_SECRET:-$(secret B2_S3_SCOPED_SECRET_ACCESS_KEY)}"

# Wasabi, with the trial account's root key. ❗ Wasabi bills every object for
# 90 days even once deleted, so its big-file cells stay small.
export CMDR_S3_LIVE_WASABI_REGION="${CMDR_S3_LIVE_WASABI_REGION:-eu-central-1}"
export CMDR_S3_LIVE_WASABI_KEY_ID="${CMDR_S3_LIVE_WASABI_KEY_ID:-$(secret WASABI_ACCESS_KEY)}"
export CMDR_S3_LIVE_WASABI_SECRET="${CMDR_S3_LIVE_WASABI_SECRET:-$(secret WASABI_SECRET_KEY)}"
export CMDR_S3_LIVE_WASABI_BUCKET="${CMDR_S3_LIVE_WASABI_BUCKET:-cmdr-s3-test-58fb74}"
export CMDR_S3_LIVE_WASABI_BUCKET_2="${CMDR_S3_LIVE_WASABI_BUCKET_2:-cmdr-s3-test-58fb74-2}"
# A third bucket in another region, for the account root's per-bucket routing.
export CMDR_S3_LIVE_WASABI_FAR_BUCKET="${CMDR_S3_LIVE_WASABI_FAR_BUCKET:-cmdr-s3-test-58fb74-euw1}"
export CMDR_S3_LIVE_WASABI_FAR_REGION="${CMDR_S3_LIVE_WASABI_FAR_REGION:-eu-west-1}"
