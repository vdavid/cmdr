# Garage: the S3 fixture with a different lineage (Rust, its own listing and
# ETag code) and NO conditional writes, so Cmdr can't quietly assume no-clobber
# PUTs work everywhere.
#
# ❗ The official image is `FROM scratch` with one binary, so there's no shell
# to run a bootstrap in. This copies that binary onto Alpine. Both pinned to an
# exact release: the cells assert on server behaviour, and a bump is a fixture
# change to review.
FROM dxflrs/garage:v2.5.0@sha256:677e5b758e4425a325b811c425c026bf35a4bf42f7ddbaa773de3f4fd779a5a5 AS garage

FROM alpine:3.24.3@sha256:be2656061b536c1883c4034790e21d0a09c5aaaf51968b89368a3c0371ea2c9a

# The bootstrap sets each bucket's lifecycle rule through the S3 API, which the
# `garage` CLI doesn't reach; curl's `--aws-sigv4` signs it.
RUN apk add --no-cache curl

COPY --from=garage /garage /usr/local/bin/garage
COPY garage.toml /etc/garage.toml
COPY entrypoint.sh /usr/local/bin/fixture-entrypoint.sh
RUN chmod +x /usr/local/bin/fixture-entrypoint.sh

EXPOSE 3900

# Healthy means "bootstrapped": layout applied, key imported, buckets allowed.
# Before that the S3 port is bound but answers every request with an error, and
# the lease adopts a stack on health.
HEALTHCHECK --interval=1s --timeout=3s --start-period=5s --retries=60 \
    CMD test -f /tmp/fixture-ready || exit 1

ENTRYPOINT ["/usr/local/bin/fixture-entrypoint.sh"]
