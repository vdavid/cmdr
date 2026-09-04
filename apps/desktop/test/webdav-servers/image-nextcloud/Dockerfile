# The sabre/dav fixture: a real Nextcloud, so the claims this backend makes about
# real servers are observations rather than RFC readings.
#
# ❗ Pinned to an exact patch release, not `stable` or `apache`: every cell here
# asserts on server behaviour (`Range`, a chunked PUT, RFC 4331 quota) and the
# `DETAILS.md` claims those cells anchor name this version. A bump is a fixture
# change to review, and the anchors move with it.
#
# SQLite, so the fixture is ONE container. Nextcloud's own guidance is a real
# database for a real deployment; what these cells read is sabre/dav's HTTP
# behaviour, which the storage engine underneath doesn't reach.
FROM nextcloud:34.0.2-apache

# Runs inside the image's own entrypoint, right after `occ maintenance:install`
# and BEFORE apache binds a port. ❗ That ordering is the whole point: a quota
# set through `occ` against a LIVE server is invisible to WebDAV until the
# process restarts (the quota storage wrapper is built once per process), so
# provisioning after the fact would make the quota cell read `-3` forever.
COPY post-install.sh /docker-entrypoint-hooks.d/post-installation/10-cmdr-fixture.sh
RUN chmod +x /docker-entrypoint-hooks.d/post-installation/10-cmdr-fixture.sh

EXPOSE 80

# Baked in so `servicesWithoutHealthcheck` stays empty and the lease library can
# wait on health rather than on "the container exists".
#
# ❗ The retries are generous because THIS container installs before it listens:
# the port stays unbound for the whole first-boot install (~35 s on a warm
# laptop, longer on a loaded CI runner), so a bound port is a true readiness
# signal and the wait has to cover the install.
HEALTHCHECK --interval=3s --timeout=3s --start-period=10s --retries=100 \
    CMD bash -c 'exec 3<>/dev/tcp/127.0.0.1/80' || exit 1
