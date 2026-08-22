# One image, eleven servers. Every fixture in this stack is a real OpenSSH
# `sshd`; what differs between them is env-driven, so a quirk is a compose line
# rather than a second image to keep in sync.
#
# Two knobs the entrypoint reads:
#
#   - AUTH / HOST_KEYS / SEED shape the server (which rungs it offers, which host
#     key types it presents, what's in `/srv/data`).
#   - QUIRK_* route the SFTP subsystem through `sftp-quirk.py`, a byte-level
#     proxy in front of the REAL `sftp-server`. That's what gives us a server
#     with no `posix-rename`, one that short-reads, and one with small `limits`
#     without swapping in a different SFTP implementation and testing something
#     other than what users run.
FROM alpine:3.21

# BOTH sshd variants. Alpine ships the PAM build as a separate `sshd.pam`
# binary, and keyboard-interactive needs PAM on Linux — that rung is one of the
# four the auth ladder has to cover, so the fixture can't skip it.
# `iproute2` is only for the bench server, which puts `tc qdisc … netem delay`
# on its own interface to measure the read window against a WAN-shaped link. One
# image for twelve servers means every container carries it; it's ~2 MB and the
# alternative is a second Dockerfile to keep in sync.
RUN apk add --no-cache openssh-server openssh-server-pam openssh-sftp-server openssh-keygen python3 shadow iproute2

COPY entrypoint.sh /usr/local/bin/entrypoint.sh
COPY sftp-quirk.py /usr/local/bin/sftp-quirk.py
COPY seed.sh /usr/local/bin/seed.sh
RUN chmod +x /usr/local/bin/entrypoint.sh /usr/local/bin/sftp-quirk.py /usr/local/bin/seed.sh

EXPOSE 22

# Baked in, so `servicesWithoutHealthcheck` stays empty and the lease library
# can wait on health rather than on "the container exists".
#
# `netstat` rather than `nc -z`: busybox's `nc` doesn't implement `-z` and
# answers 1 unconditionally, which reads as a container that never comes up.
HEALTHCHECK --interval=2s --timeout=3s --start-period=5s --retries=60 \
    CMD netstat -lnt | grep -q ':22 ' || exit 1

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
