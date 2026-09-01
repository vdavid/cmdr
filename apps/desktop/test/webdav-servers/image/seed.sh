#!/bin/bash
# Materializes one fixture server's export.
#
# ❗ Both servers carry the same landmarks so a cell can assert on the same
# names whichever server it is pointed at.
set -e
root="$1"

mkdir -p "$root/docs" "$root/nested/deep" "$root/many" "$root/empty" "$root/photos/2024 summer"
printf 'hello\n' > "$root/hello.txt"
printf '# readme\n' > "$root/docs/readme.md"
printf 'deep\n' > "$root/nested/deep/file.txt"
printf 'ok\n' > "$root/naïve name.txt"
printf 'sun\n' > "$root/photos/2024 summer/beach.txt"

# A directory big enough that a listing is real work for the multistatus parser.
i=0
while [ "$i" -lt 300 ]; do
    printf 'entry %s\n' "$i" > "$root/many/file-$i.txt"
    i=$((i + 1))
done

# ── The file the byte path reads ─────────────────────────────────────
#
# Self-describing by construction: every 16-byte line holds its own line number,
# zero-padded, so each position in the file says where it belongs. A reader that
# holes or duplicates a chunk lands bytes at offsets that no longer match their
# own contents, which is what lets a cell assert byte-exactness without shipping
# a copy of the file next to the test.
# `cmdr_webdav::volume::testing::fixture_large_bytes` regenerates the expectation.
#
# `awk` rather than Python: the httpd image ships no interpreter, and awk's
# `printf` is enough for a zero-padded counter.
large_mb="${LARGE_MB:-4}"
awk -v lines=$((large_mb * 65536)) 'BEGIN { for (i = 0; i < lines; i++) printf "%015d\n", i }' > "$root/large.bin"
