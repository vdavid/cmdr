#!/bin/bash
# Configures one fixture server from its env, seeds its export, and runs httpd
# in the foreground.
#
# Env, all optional:
#   AUTH        basic | digest      (default: basic)
#   RANGES      honour | ignore     (default: honour)
#   LARGE_MB    size of `large.bin` in MiB (default: 4)
set -e

USER_NAME="${USER_NAME:-ada}"
USER_PASSWORD="${USER_PASSWORD:-openthedoor}"
AUTH="${AUTH:-basic}"
RANGES="${RANGES:-honour}"
REALM="cmdr"
EXPORT_DIR=/srv/data
HTTPD_PREFIX=/usr/local/apache2
CONF="$HTTPD_PREFIX/conf/httpd.conf"

# Everything below the guard runs once per container: `restart: unless-stopped`
# re-runs this script on the same filesystem, and a second round of `sed -i`
# over `httpd.conf` would double every `Include`.
if [ -f /etc/fixture-configured ]; then
    exec httpd-foreground
fi

# ── Modules ──────────────────────────────────────────────────────────
#
# The stock `httpd.conf` compiles every module in but leaves the DAV pair and
# Digest commented out; Basic, `authn_file`, and the authz core are on already.
sed -i \
    -e 's|^#\(LoadModule dav_module .*\)|\1|' \
    -e 's|^#\(LoadModule dav_fs_module .*\)|\1|' \
    -e 's|^#\(LoadModule auth_digest_module .*\)|\1|' \
    "$CONF"

# ── Credentials ──────────────────────────────────────────────────────
#
# Generated at start rather than baked into the image so the same image serves
# both schemes. `htdigest` is interactive, so the digest line is computed by
# hand: `user:realm:MD5(user:realm:password)` is its whole file format.
htpasswd -nbB "$USER_NAME" "$USER_PASSWORD" > /etc/webdav.htpasswd
ha1=$(printf '%s:%s:%s' "$USER_NAME" "$REALM" "$USER_PASSWORD" | md5sum | cut -d' ' -f1)
printf '%s:%s:%s\n' "$USER_NAME" "$REALM" "$ha1" > /etc/webdav.htdigest

case "$AUTH" in
    basic)
        auth_block="        AuthType Basic
        AuthName \"$REALM\"
        AuthBasicProvider file
        AuthUserFile /etc/webdav.htpasswd"
        ;;
    digest)
        # Digest ONLY: a client offering Basic here gets a 401 whose
        # `WWW-Authenticate` names no scheme it can answer with, which is the
        # typed refusal this server exists to provoke.
        auth_block="        AuthType Digest
        AuthName \"$REALM\"
        AuthDigestProvider file
        AuthUserFile /etc/webdav.htdigest"
        ;;
    *) echo "unknown AUTH: $AUTH" >&2; exit 1 ;;
esac

# ── Range handling ───────────────────────────────────────────────────
#
# RFC 9110 § 14.2 makes ranges optional, so a client has to survive a server
# that answers a ranged GET with 200 and the whole resource. `MaxRanges none`
# is how Apache says exactly that: the core handler drops every `Range` header
# and serves the entire file. It is a core directive, so this needs no extra
# module loaded, and it leaves everything else about the server alone — which
# is what makes the difference between this service and the stock one one line.
#
# ❗ Not `RequestHeader unset Range`: that works too, but it needs
# `mod_headers` uncommented and it hides the header from the whole request
# chain rather than telling the range machinery to stand down.
case "$RANGES" in
    honour) ranges_directive="" ;;
    ignore) ranges_directive="    MaxRanges none" ;;
    *) echo "unknown RANGES: $RANGES" >&2; exit 1 ;;
esac

# ── The export ───────────────────────────────────────────────────────
#
# ❗ `DavLockDB` has to live somewhere the httpd WORKER user can write, and
# `mod_dav` opens that database before every write method (to evaluate a
# possible `If:` header), not just on LOCK. A directory the worker can't write
# turns every PUT, MKCOL, MOVE, COPY, and DELETE into a 500 carrying
# "Could not open the lock database" in the error log, which reads like a
# backend bug. The user is read from `httpd.conf` below rather than hardcoded:
# the stock image runs as `www-data`, and guessing `daemon` is exactly how this
# broke once.
#
# `DavDepthInfinity On` because a client that lists a whole tree in one PROPFIND
# is what the crate does; `DavMinTimeout 600` so a lock a cell takes outlives
# the cell.
mkdir -p "$HTTPD_PREFIX/var"
cat > "$HTTPD_PREFIX/conf/webdav-fixture.conf" <<CONF
DavLockDB $HTTPD_PREFIX/var/DavLock
DavDepthInfinity On
DavMinTimeout 600

# A whole test binary's cells run in parallel against one server.
MaxRequestWorkers 150

Alias /dav $EXPORT_DIR
<Directory "$EXPORT_DIR">
    Dav On
    Options +Indexes
    AllowOverride None
$ranges_directive
$auth_block
    Require valid-user
</Directory>
CONF
echo 'Include conf/webdav-fixture.conf' >> "$CONF"

/usr/local/bin/seed.sh "$EXPORT_DIR"

# The user httpd drops to after binding the port, straight from its own config,
# so an image that changes it can't leave the export read-only behind our back.
run_user=$(awk '$1 == "User" { print $2 }' "$CONF" | tail -n 1)
run_group=$(awk '$1 == "Group" { print $2 }' "$CONF" | tail -n 1)
chown -R "${run_user:-www-data}:${run_group:-www-data}" "$EXPORT_DIR" "$HTTPD_PREFIX/var"

touch /etc/fixture-configured
exec httpd-foreground
