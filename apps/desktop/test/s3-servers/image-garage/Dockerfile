# Garage: the S3 fixture with a different lineage (Rust, its own listing and
# ETag code) and NO conditional writes, so Cmdr can't quietly assume no-clobber
# PUTs work everywhere.
#
# ❗ The official image is `FROM scratch` with one binary, so there's no shell
# to run a bootstrap in. This copies that binary onto Alpine. Both pinned to an
# exact release: the cells assert on server behaviour, and a bump is a fixture
# change to review.
FROM dxflrs/garage:v2.4.1@sha256:9c96caa2612d3411acc5b0e6701fb238dbfba33e533a6d7d3d811a4b12d0d020 AS garage

FROM alpine:3.24.2@sha256:294b683cb724975bec92580e1e685676bd4b50bda910ddb8c51d4cabeaec77e6

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
