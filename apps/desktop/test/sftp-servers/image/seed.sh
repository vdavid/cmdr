#!/bin/sh
# Materializes one fixture server's export.
#
# ❗ Every profile keeps `hello.txt` and `photos/` so a cell can assert on the
# same landmark whichever server it happens to be pointed at.
set -e
profile="$1"
root="$2"
owner="$3"

mkdir -p "$root/photos"
printf 'hello from the sftp fixture\n' > "$root/hello.txt"
printf 'a smaller file\n' > "$root/photos/note.txt"
# Two files that differ in size, which is the precondition the shared rename
# conformance assertion checks before it trusts its own result.
printf '0123456789' > "$root/ten-bytes.txt"
printf '01234' > "$root/five-bytes.txt"
mkdir -p "$root/empty-dir"
mkdir -p "$root/full-dir" && printf 'x' > "$root/full-dir/child.txt"

# ── The file the byte path reads ─────────────────────────────────────
#
# Self-describing by construction: every 16-byte line holds its own line number,
# zero-padded, so each position in the file says where it belongs. A reader that
# holes or duplicates a chunk lands bytes at offsets that no longer match their
# own contents, which is what lets a cell assert byte-exactness without shipping
# a copy of the file next to the test.
#
# `LARGE_MB` is 4 by default — enough that a 255 KiB window has real work and a
# short-reading server has to loop — and the bench server raises it to something
# a throughput number can be measured over.
large_mb="${LARGE_MB:-4}"
python3 - "$root/large.bin" "$large_mb" <<'GENERATE'
import sys

path, mib = sys.argv[1], int(sys.argv[2])
LINES_PER_MIB = 1024 * 1024 // 16
with open(path, "wb") as out:
    for base in range(0, mib * LINES_PER_MIB, LINES_PER_MIB):
        out.write(b"".join(b"%015d\n" % line for line in range(base, base + LINES_PER_MIB)))
GENERATE

case "$profile" in
    small) ;;
    big)
        # A directory big enough that a listing is many round trips, and a nest
        # deep enough that a per-level `exists()` walk is visibly the wrong
        # shape.
        mkdir -p "$root/many"
        i=0
        while [ "$i" -lt 5000 ]; do
            printf 'entry %s\n' "$i" > "$root/many/file-$i.txt"
            i=$((i + 1))
        done
        deep="$root/deep"
        i=0
        while [ "$i" -lt 40 ]; do
            deep="$deep/level-$i"
            i=$((i + 1))
        done
        mkdir -p "$deep"
        printf 'bottom\n' > "$deep/bottom.txt"
        ;;
    oddnames)
        # SFTP v3 filenames are BYTES with no declared encoding, which is new for
        # Cmdr: SMB is UTF-16 on the wire. `openssh-sftp-client` fails the whole
        # readdir on a name that isn't UTF-8, which is the loud failure this
        # directory exists to pin.
        mkdir -p "$root/utf8" "$root/latin1"
        printf 'ok\n' > "$root/utf8/naïve — résumé.txt"
        printf 'ok\n' > "$root/utf8/🦀.txt"
        printf 'ok\n' > "$root/utf8/a b  c.txt"
        # 0xE9 is `é` in latin-1 and is not valid UTF-8 on its own.
        python3 -c "open(b'$root/latin1/caf\xe9.txt', 'w').write('ok')"
        ;;
    *) echo "unknown seed profile: $profile" >&2; exit 1 ;;
esac

chown -R "$owner":"$owner" "$root"
