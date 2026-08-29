#!/bin/sh
# Configures one fixture server from its env, seeds its export, and runs sshd in
# the foreground.
#
# Env, all optional:
#   AUTH        password | key | passphrase | keyboard-interactive  (default: password+key)
#   HOST_KEYS   comma-separated: ed25519, rsa                       (default: ed25519)
#   SEED        small | big | oddnames                              (default: small)
#   QUIRK_*     see sftp-quirk.py
set -e

USER_NAME="${USER_NAME:-ada}"
USER_PASSWORD="${USER_PASSWORD:-openthedoor}"
AUTH="${AUTH:-password+key}"
HOST_KEYS="${HOST_KEYS:-ed25519}"
SEED="${SEED:-small}"
EXPORT_DIR=/srv/data

# Alpine ships the PAM build as its own binary, and only the
# keyboard-interactive fixture needs it.
sshd_binary() {
    if [ "$AUTH" = "keyboard-interactive" ]; then
        echo /usr/sbin/sshd.pam
    else
        echo /usr/sbin/sshd
    fi
}

# Which servers accept a client key at all, so which ones have a `/keys` bind
# mount to fill. ❗ An explicit list: a `case "$AUTH" in *key*)` glob also matches
# `keyboard-interactive`, which offers `PubkeyAuthentication no` and mounts
# nothing, and would leave a pair nobody can reach in a container-local `/keys`.
offers_key_auth() {
    case "$AUTH" in
        key | passphrase | password+key) return 0 ;;
        *) return 1 ;;
    esac
}

# ── The client key every key-auth fixture accepts ────────────────────
#
# Generated at start rather than baked into the image, because a private key in
# the repo is a private key on the internet. The private half lands in `/keys`,
# the bind mount the suite reads it back from.
#
# ❗ Re-checked on EVERY start, ❌ never once. `/keys` is host state under `/tmp`,
# which macOS empties on reboot, while this container's own filesystem — the
# `authorized_keys` and the `/etc/fixture-configured` guard below — survives it.
# The server is then left naming a public key whose private half exists nowhere,
# and it does that while `docker ps` and the HEALTHCHECK both call it healthy: no
# rung answers, so `sftp-fixture-keyonly` reports `NeedsCredentials` and
# `sftp-fixture-passphrase` falls through to the password rung it refuses and
# reports `AuthenticationRejected`. Comparing the pair against `authorized_keys`
# on every start is what lets a plain restart heal that.
provision_client_key() {
    authorized="/home/$USER_NAME/.ssh/authorized_keys"
    mkdir -p /keys "/home/$USER_NAME/.ssh"
    chmod 700 "/home/$USER_NAME/.ssh"

    # Byte-equal, because `authorized_keys` is a copy of the public half: this
    # catches a half-written pair and a pair from a previous container just as
    # well as an empty mount.
    if [ -f /keys/id_ed25519 ] && cmp -s /keys/id_ed25519.pub "$authorized"; then
        return 0
    fi

    # `ssh-keygen` stops on an interactive overwrite prompt, so clear whatever is
    # there before generating.
    rm -f /keys/id_ed25519 /keys/id_ed25519.pub
    if [ "$AUTH" = "passphrase" ]; then
        ssh-keygen -q -t ed25519 -N "${KEY_PASSPHRASE:-letmein}" -f /keys/id_ed25519
    else
        ssh-keygen -q -t ed25519 -N '' -f /keys/id_ed25519
    fi
    cp /keys/id_ed25519.pub "$authorized"
    chmod 600 "$authorized"
    # ❗ World-readable, deliberately. `/keys` is a bind mount, the container runs
    # as root, and the integration lane runs on Linux — where a 600 root-owned
    # file is unreadable to the test process that has to load it. It's a
    # throwaway key generated at container start for a server reachable only on
    # localhost, and nothing here checks file modes the way `ssh` does.
    chmod 644 /keys/id_ed25519 /keys/id_ed25519.pub
    chown -R "$USER_NAME":"$USER_NAME" "/home/$USER_NAME/.ssh"
    echo "provisioned a fresh client key pair for AUTH=$AUTH" >&2
}

# Everything below the guard runs once per container: `restart: unless-stopped`
# re-runs this script on the same filesystem, and a second `adduser` or
# `ssh-keygen -f <existing>` would abort under `set -e`. The key pair is the one
# thing that is NOT once-per-container, for the reason above.
if [ -f /etc/fixture-configured ]; then
    if offers_key_auth; then provision_client_key; fi
    exec "$(sshd_binary)" -D -e
fi

adduser -D -h /home/"$USER_NAME" "$USER_NAME"
echo "$USER_NAME:$USER_PASSWORD" | chpasswd

mkdir -p /etc/ssh /var/empty /home/"$USER_NAME"/.ssh
chmod 700 /home/"$USER_NAME"/.ssh

# ── Host keys ────────────────────────────────────────────────────────
#
# Generated fresh per container, so every service has an identity of its own and
# nothing is checked into the repo. Suites approve on first contact the way a
# user does, and the changed-key cell reads one server's real fingerprint and
# offers it against another server's address.
for kind in $(echo "$HOST_KEYS" | tr ',' ' '); do
    key="/etc/ssh/ssh_host_${kind}_key"
    case "$kind" in
        ed25519) ssh-keygen -q -t ed25519 -N '' -f "$key" ;;
        rsa)     ssh-keygen -q -t rsa -b 2048 -N '' -f "$key" ;;
        *)       echo "unknown host key kind: $kind" >&2; exit 1 ;;
    esac
    echo "HostKey $key" >> /etc/ssh/sshd_config.fixture
done

if offers_key_auth; then provision_client_key; fi

# ── Which rungs this server offers ───────────────────────────────────
case "$AUTH" in
    password)
        echo "PasswordAuthentication yes" >> /etc/ssh/sshd_config.fixture
        echo "PubkeyAuthentication no" >> /etc/ssh/sshd_config.fixture
        echo "KbdInteractiveAuthentication no" >> /etc/ssh/sshd_config.fixture
        ;;
    key|passphrase)
        echo "PasswordAuthentication no" >> /etc/ssh/sshd_config.fixture
        echo "PubkeyAuthentication yes" >> /etc/ssh/sshd_config.fixture
        echo "KbdInteractiveAuthentication no" >> /etc/ssh/sshd_config.fixture
        ;;
    keyboard-interactive)
        # The `PasswordAuthentication no` + `KbdInteractiveAuthentication yes`
        # shape, which is what a hardened server without 2FA looks like. PAM asks
        # exactly one hidden question, which is the single-prompt case the
        # backend answers without a human.
        echo "PasswordAuthentication no" >> /etc/ssh/sshd_config.fixture
        echo "PubkeyAuthentication no" >> /etc/ssh/sshd_config.fixture
        echo "KbdInteractiveAuthentication yes" >> /etc/ssh/sshd_config.fixture
        echo "UsePAM yes" >> /etc/ssh/sshd_config.fixture
        ;;
    password+key)
        echo "PasswordAuthentication yes" >> /etc/ssh/sshd_config.fixture
        echo "PubkeyAuthentication yes" >> /etc/ssh/sshd_config.fixture
        echo "KbdInteractiveAuthentication no" >> /etc/ssh/sshd_config.fixture
        ;;
    *) echo "unknown AUTH: $AUTH" >&2; exit 1 ;;
esac

# ── The SFTP subsystem, straight or through the quirk proxy ──────────
if [ -n "$QUIRK_DROP_EXTENSIONS$QUIRK_SHORT_READ_BYTES$QUIRK_LIMITS" ]; then
    echo "Subsystem sftp /usr/local/bin/sftp-quirk.py" >> /etc/ssh/sshd_config.fixture
    # sshd scrubs the environment for the subsystem, so the quirks travel in a
    # file the proxy reads instead.
    : > /etc/sftp-quirks
    [ -n "$QUIRK_DROP_EXTENSIONS" ] && echo "QUIRK_DROP_EXTENSIONS=$QUIRK_DROP_EXTENSIONS" >> /etc/sftp-quirks
    [ -n "$QUIRK_SHORT_READ_BYTES" ] && echo "QUIRK_SHORT_READ_BYTES=$QUIRK_SHORT_READ_BYTES" >> /etc/sftp-quirks
    [ -n "$QUIRK_LIMITS" ] && echo "QUIRK_LIMITS=$QUIRK_LIMITS" >> /etc/sftp-quirks
else
    echo "Subsystem sftp /usr/lib/ssh/sftp-server" >> /etc/ssh/sshd_config.fixture
fi

cat >> /etc/ssh/sshd_config.fixture <<CONF
Port 22
ListenAddress 0.0.0.0
PermitRootLogin no
AllowUsers $USER_NAME
StrictModes no
PidFile /run/sshd.pid
LogLevel INFO
# A fixture serves a whole test binary at once, and nextest runs its cells in
# parallel. OpenSSH's stock `MaxStartups 10:30:100` starts refusing the 11th
# unauthenticated connection, which surfaces as a flaky "Disconnected" rather
# than as anything a test is about.
MaxStartups 200:30:400
MaxSessions 100
LoginGraceTime 30
# ❗ Both off, and this is the one that bites. OpenSSH 9.8 added per-source
# PENALTIES: one failed authentication blocks the whole source address for a
# while, so the cell that deliberately signs in with the wrong password takes
# every other cell on that server down with it — as an unrelated
# "Disconnected", from a different test, seconds later.
PerSourcePenalties no
PerSourceMaxStartups none
CONF
mv /etc/ssh/sshd_config.fixture /etc/ssh/sshd_config

/usr/local/bin/seed.sh "$SEED" "$EXPORT_DIR" "$USER_NAME"

touch /etc/fixture-configured
exec "$(sshd_binary)" -D -e
