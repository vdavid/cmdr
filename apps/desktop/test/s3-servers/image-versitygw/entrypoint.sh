#!/bin/sh
# Runs VersityGW over /data and, beside it, creates the fixture buckets through
# the S3 API. Idempotent: `restart: unless-stopped` and a persistent volume mean
# this runs again over buckets that already exist.
#
# Env (the compose file sets all of them):
#   S3_ACCESS_KEY, S3_SECRET_KEY   the root credentials
#   S3_BUCKETS                     space-separated bucket names to ensure
#   FIXTURE_PREFIX_MAX_AGE_MIN     optional; scratch prefixes older than this go (default 120)
#   FIXTURE_JANITOR_INTERVAL_S     optional; how often the janitor looks (default 600)
set -e

: "${S3_ACCESS_KEY:?}" "${S3_SECRET_KEY:?}" "${S3_BUCKETS:?}"
PORT=7070
DATA_DIR=/data
READY=/tmp/fixture-ready
MAX_AGE_MIN="${FIXTURE_PREFIX_MAX_AGE_MIN:-120}"
INTERVAL_S="${FIXTURE_JANITOR_INTERVAL_S:-600}"

# A marker from the previous run would read as healthy before the gateway is up.
rm -f "$READY"
mkdir -p "$DATA_DIR"

bootstrap() {
    # The gateway's own health path, unsigned. ❌ No fixed sleep: poll.
    tries=0
    until wget -q -O /dev/null "http://127.0.0.1:$PORT/health"; do
        tries=$((tries + 1))
        if [ "$tries" -ge 300 ]; then
            echo "fixture: VersityGW never answered /health" >&2
            return 1
        fi
        sleep 0.1
    done

    for bucket in $S3_BUCKETS; do
        # 200 creates it; 409 `BucketAlreadyOwnedByYou` is the re-run.
        code=$(curl -s -o /dev/null -w '%{http_code}' -X PUT \
            --aws-sigv4 "aws:amz:us-east-1:s3" --user "$S3_ACCESS_KEY:$S3_SECRET_KEY" \
            "http://127.0.0.1:$PORT/$bucket")
        case "$code" in
            200 | 409) echo "fixture: bucket $bucket ready ($code)" ;;
            *)
                echo "fixture: creating bucket $bucket answered $code" >&2
                return 1
                ;;
        esac
    done
    touch "$READY"
}


# Every cell writes under a `scratch_prefix` of its own and never deletes it, so
# without this the volume only grows: 4,900 prefixes, 64 GB, in five days. A
# prefix is a top-level directory in its bucket (the POSIX backend), and its
# mtime moves whenever a key lands directly under it, so an untouched one older
# than any run is safe to drop. Straight off the disk rather than through the
# API: one `rm` per prefix instead of one DELETE per key. Only `cmdr-test-*`
# names, so the gateway's own dot-dirs (`.vgwlocks`) and the seeds shared across
# runs (`cmdr-seed-*`) are never touched.
janitor() {
    while :; do
        for bucket in $S3_BUCKETS; do
            [ -d "$DATA_DIR/$bucket" ] || continue
            find "$DATA_DIR/$bucket" -mindepth 1 -maxdepth 1 -type d -name 'cmdr-test-*' \
                -mmin "+$MAX_AGE_MIN" -exec rm -rf {} + 2> /dev/null || true
        done
        sleep "$INTERVAL_S"
    done
}

bootstrap &
janitor &

# PID 1 is the gateway itself, so `docker stop` signals it directly.
exec versitygw --port ":$PORT" --health /health --quiet \
    --access "$S3_ACCESS_KEY" --secret "$S3_SECRET_KEY" \
    posix "$DATA_DIR"
