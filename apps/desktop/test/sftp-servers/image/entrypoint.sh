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

# Idempotent: `restart: unless-stopped` re-runs this on the same filesystem, and
# a second `adduser` or `ssh-keygen -f <existing>` would abort under `set -e`.
# Alpine ships the PAM build as its own binary, and only the
# keyboard-interactive fixture needs it.
sshd_binary() {
    if [ "${AUTH:-password+key}" = "keyboard-interactive" ]; then
        echo /usr/sbin/sshd.pam
    else
        echo /usr/sbin/sshd
    fi
}

if [ -f /etc/fixture-configured ]; then
    exec "$(sshd_binary)" -D -e
fi

USER_NAME="${USER_NAME:-ada}"
USER_PASSWORD="${USER_PASSWORD:-openthedoor}"
AUTH="${AUTH:-password+key}"
HOST_KEYS="${HOST_KEYS:-ed25519}"
SEED="${SEED:-small}"
EXPORT_DIR=/srv/data

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

# ── The client key every key-auth fixture accepts ────────────────────
#
# Baked into the image at build time would mean a private key in the repo; this
# generates the pair at start and writes the private half where the suite can
# read it, on the volume the compose file shares with the host.
if [ "${AUTH#*key}" != "$AUTH" ] || [ "$AUTH" = "passphrase" ]; then
    mkdir -p /keys
    # `/keys` is bind-mounted from the host, so a previous run's pair is still
    # there; ssh-keygen would stop on an interactive overwrite prompt.
    rm -f /keys/id_ed25519 /keys/id_ed25519.pub
    if [ "$AUTH" = "passphrase" ]; then
        ssh-keygen -q -t ed25519 -N "${KEY_PASSPHRASE:-letmein}" -f /keys/id_ed25519
    else
        ssh-keygen -q -t ed25519 -N '' -f /keys/id_ed25519
    fi
    cp /keys/id_ed25519.pub /home/"$USER_NAME"/.ssh/authorized_keys
    chmod 600 /home/"$USER_NAME"/.ssh/authorized_keys
    # ❗ World-readable, deliberately. `/keys` is a bind mount, the container runs
    # as root, and the integration lane runs on Linux — where a 600 root-owned
    # file is unreadable to the test process that has to load it. It's a
    # throwaway key generated at container start for a server reachable only on
    # localhost, and nothing here checks file modes the way `ssh` does.
    chmod 644 /keys/id_ed25519 /keys/id_ed25519.pub
    chown -R "$USER_NAME":"$USER_NAME" /home/"$USER_NAME"/.ssh
fi

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
