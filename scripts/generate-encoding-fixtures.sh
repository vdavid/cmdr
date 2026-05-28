#!/usr/bin/env bash
# Generate encoding fixtures for the file viewer tests.
#
# Output: apps/desktop/test/fixtures/encodings/
#   - utf8.txt                  : plain ASCII
#   - utf8-with-bom.txt         : EF BB BF prefix + ASCII
#   - utf8-cjk.txt              : Japanese ("日本語のテキスト") to exercise the
#                                 multibyte UTF-8 path
#   - windows-1252.txt          : "café\n" using the 0xE9 single-byte é
#   - utf16-le.txt              : "hello world\n" repeated; no BOM (parity-detected)
#   - utf16-le-bom.txt          : same payload with the FF FE BOM
#   - utf16-be.txt              : same payload, big-endian, no BOM
#   - utf16-be-bom.txt          : same payload with the FE FF BOM
#   - large-utf16-le-6mb.txt    : 6 MB of UTF-16 LE "hello world\n" lines for the
#                                 Playwright spec (kept out of git; regenerate on demand)
#
# The script is idempotent: rerun any time to refresh the fixtures.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
OUT_DIR="${REPO_ROOT}/apps/desktop/test/fixtures/encodings"

mkdir -p "${OUT_DIR}"

write_utf8_ascii() {
    printf 'line one\nline two\nline three\n' > "${OUT_DIR}/utf8.txt"
}

write_utf8_with_bom() {
    {
        printf '\xEF\xBB\xBF'
        printf 'line one\nline two\nline three\n'
    } > "${OUT_DIR}/utf8-with-bom.txt"
}

write_utf8_cjk() {
    printf '日本語のテキスト\n二行目\n' > "${OUT_DIR}/utf8-cjk.txt"
}

write_windows_1252() {
    # 0xE9 is é in Windows-1252 / ISO-8859-1, but invalid UTF-8.
    printf 'caf\xE9\nna\xEFvet\xE9\n' > "${OUT_DIR}/windows-1252.txt"
}

# Encode the given UTF-8 string into UTF-16 with the chosen endianness and an
# optional BOM. Uses Python because POSIX shells lack a portable iconv flag set.
encode_utf16() {
    local payload="$1"
    local endian="$2"    # le or be
    local with_bom="$3"  # bom or no-bom
    local out="$4"
    python3 - "$payload" "$endian" "$with_bom" "$out" <<'PY'
import sys
payload, endian, with_bom, out = sys.argv[1:5]
data = b''
if with_bom == 'bom':
    data += b'\xff\xfe' if endian == 'le' else b'\xfe\xff'
codec = 'utf-16-le' if endian == 'le' else 'utf-16-be'
data += payload.encode(codec)
with open(out, 'wb') as f:
    f.write(data)
PY
}

write_utf16_fixtures() {
    local payload='hello world\nsecond line\n'
    encode_utf16 "$(printf '%b' "$payload")" le no-bom "${OUT_DIR}/utf16-le.txt"
    encode_utf16 "$(printf '%b' "$payload")" le bom "${OUT_DIR}/utf16-le-bom.txt"
    encode_utf16 "$(printf '%b' "$payload")" be no-bom "${OUT_DIR}/utf16-be.txt"
    encode_utf16 "$(printf '%b' "$payload")" be bom "${OUT_DIR}/utf16-be-bom.txt"
}

write_large_utf16_le() {
    # Roughly 6 MB of UTF-16 LE "hello world\n" lines. Used by the Playwright
    # encoding-picker spec. Excluded from git via the encodings/.gitignore below.
    local out="${OUT_DIR}/large-utf16-le-6mb.txt"
    python3 - "$out" <<'PY'
import sys
out = sys.argv[1]
line = 'hello world\n'.encode('utf-16-le')
target = 6 * 1024 * 1024
repeats = (target + len(line) - 1) // len(line)
with open(out, 'wb') as f:
    f.write(line * repeats)
PY
}

write_utf8_ascii
write_utf8_with_bom
write_utf8_cjk
write_windows_1252
write_utf16_fixtures
write_large_utf16_le

# Keep the large fixture out of git to avoid bloating the repo. The smaller
# fixtures are checked in so vitest / nextest don't need to regenerate them.
GITIGNORE="${OUT_DIR}/.gitignore"
if [ ! -f "${GITIGNORE}" ]; then
    cat > "${GITIGNORE}" <<'EOF'
# Large UTF-16 fixture is regenerated on demand by
# scripts/generate-encoding-fixtures.sh — too big to commit.
large-utf16-le-6mb.txt
EOF
fi

echo "Encoding fixtures written to ${OUT_DIR}"
