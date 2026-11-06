# The browser demo bundle as an image: nothing but the files of `apps/desktop/build-demo/`, at
# its root. The release pipeline's `demo` job builds it with that dir as the context and pushes it to
# `ghcr.io/vdavid/cmdr-demo:<release tag>`; the website's Dockerfile copies it into the site, so
# the server never builds Rust. `apps/website/DETAILS.md` § Browser demo.
FROM scratch
LABEL org.opencontainers.image.source="https://github.com/vdavid/cmdr"
LABEL org.opencontainers.image.description="Cmdr's browser demo bundle, copied into getcmdr.com/demo/ at deploy"
COPY . /
