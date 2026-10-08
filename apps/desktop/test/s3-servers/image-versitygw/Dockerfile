# VersityGW over its POSIX backend: the S3 fixture that behaves like AWS on
# conditional writes (`If-None-Match: *` answers 412 on a clash).
#
# ❗ Pinned to an exact release, not `latest`: the cells assert on server
# behaviour (conditional writes, multipart, `UploadPartCopy`), and a bump is a
# fixture change to review, not to inherit. The README records what this
# version answered.
FROM versity/versitygw:v1.8.0@sha256:30292fc2eeacc67a36993b01f7a7a5e3361a19cced0e80c1d71cfa2a4b0a2499

# The base image is Alpine with `wget` only. The bootstrap needs a SigV4-signing
# client to create the buckets through the API (a bucket made by `mkdir` lacks
# the ownership xattrs the gateway writes), and curl's `--aws-sigv4` is one.
RUN apk add --no-cache curl

COPY entrypoint.sh /usr/local/bin/fixture-entrypoint.sh
RUN chmod +x /usr/local/bin/fixture-entrypoint.sh

EXPOSE 7070

# Healthy means "bootstrapped", not "port bound": the lease adopts a stack on
# health, and a gateway that answers before its buckets exist would hand a cell
# a `NoSuchBucket`. The entrypoint writes the marker after the last bucket.
HEALTHCHECK --interval=1s --timeout=3s --start-period=5s --retries=60 \
    CMD test -f /tmp/fixture-ready && wget -q -O /dev/null http://127.0.0.1:7070/health || exit 1

ENTRYPOINT ["/usr/local/bin/fixture-entrypoint.sh"]
