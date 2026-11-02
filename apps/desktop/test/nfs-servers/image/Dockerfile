# One image, one server: the Linux kernel's own NFS server (knfsd), the one every
# QNAP, Synology, and TrueNAS runs. `nfs-utils` is only its userland (`exportfs`,
# `rpc.mountd`, `rpc.nfsd`); the server itself is the host kernel's `nfsd`, which
# is why the service runs privileged (see the README, § "Why knfsd").
#
# ❗ Pinned to the same Alpine as the SFTP fixture. The cells assert on server
# behaviour (READDIRPLUS, the per-call `secure` check, wcc_data), and a base bump
# is a fixture change to review, not to inherit.
FROM alpine:3.24@sha256:be2656061b536c1883c4034790e21d0a09c5aaaf51968b89368a3c0371ea2c9a

# `python3` writes `large.bin` and the one name that isn't UTF-8.
RUN apk add --no-cache nfs-utils rpcbind python3

COPY entrypoint.sh /usr/local/bin/entrypoint.sh
COPY seed.sh /usr/local/bin/seed.sh
RUN chmod +x /usr/local/bin/entrypoint.sh /usr/local/bin/seed.sh

# Baked in, so `servicesWithoutHealthcheck` stays empty and the lease library
# can wait on health rather than on "the container exists". ❗ Healthy means
# portmap lists MOUNT v3 and NFS v3 over TCP AND the seed finished: a bound port
# alone comes up before `exportfs` has run, and a cell would then read an empty
# export list as a server bug.
HEALTHCHECK --interval=2s --timeout=3s --start-period=10s --retries=90 \
    CMD test -f /run/nfs-fixture-ready && rpcinfo -p 127.0.0.1 | grep -E '^ +100005 +3 +tcp' >/dev/null \
        && rpcinfo -p 127.0.0.1 | grep -E '^ +100003 +3 +tcp' >/dev/null || exit 1

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
