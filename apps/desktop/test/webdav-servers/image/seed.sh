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

# A file dated years back (2021-01-29 08:30:15 UTC, `conformance::SOURCE_DATE_SECS`).
# ❗ `mod_dav` has no way to SET a date over the wire, so this is the only way a
# cell gets a file whose date can't be mistaken for "now": the read stream's
# date cells copy it off and insist the date travels.
# `cmdr_webdav::volume::testing::FIXTURE_DATED_FILE` names it.
printf 'from 2021\n' > "$root/dated.txt"
touch -d @1611909015 "$root/dated.txt"

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
