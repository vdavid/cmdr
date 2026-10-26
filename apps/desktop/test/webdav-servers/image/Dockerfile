# One image, two servers. Every fixture in this stack is the official Apache
# httpd with `mod_dav` + `mod_dav_fs`; what differs between them is env-driven
# (`AUTH=basic|digest`), so a scheme is a compose line rather than a second image
# to keep in sync.
#
# ❗ Pinned to the 2.4 line, not `latest`: the cells assert on server behaviour
# (COPY/MOVE semantics, `Depth: infinity` PROPFIND, Range on GET), and a major
# bump is a fixture change to review, not to inherit.
FROM httpd:2.4@sha256:a30cd4cd57f3f3c3be9b2d16384d2488839996bb14c67df2b51f1ecbb3e584a1

COPY entrypoint.sh /usr/local/bin/entrypoint.sh
COPY seed.sh /usr/local/bin/seed.sh
RUN chmod +x /usr/local/bin/entrypoint.sh /usr/local/bin/seed.sh

EXPOSE 80

# Baked in, so `servicesWithoutHealthcheck` stays empty and the lease library
# can wait on health rather than on "the container exists".
#
# `/dev/tcp` rather than `curl`/`wget`: the httpd image ships neither, and bash
# is enough to tell a bound port from an unbound one.
HEALTHCHECK --interval=2s --timeout=3s --start-period=5s --retries=60 \
    CMD bash -c 'exec 3<>/dev/tcp/127.0.0.1/80' || exit 1

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
