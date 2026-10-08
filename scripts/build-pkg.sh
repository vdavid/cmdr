#!/bin/bash
# Builds the installer package (`.pkg`) for MDM deployment from a finished, signed `Cmdr.app`:
# a component package that installs the app to /Applications (not relocatable), wrapped in a
# distribution package. Optionally signs it with a Developer ID Installer identity, notarizes, and
# staples it. Then it checks what it built. The release pipeline's `pkg` job calls it on the app
# from the published universal DMG; run it locally on any build to test the packaging itself.
# Design and decisions: `docs/guides/releasing.md` § The installer package.
#
# Usage: ./scripts/build-pkg.sh [--app PATH] [--out DIR] [--sign IDENTITY [--keychain PATH]] [--notarize]
#   --app       the app bundle to package (default: /Applications/Cmdr.app)
#   --out       where the .pkg goes (default: a new temp dir, printed at the end)
#   --sign      "Developer ID Installer: …" identity; unsigned without it
#   --keychain  the keychain holding that identity (default: the search list)
#   --notarize  submit to Apple, wait, and staple (needs --sign, plus APPLE_API_KEY,
#               APPLE_API_ISSUER, and APPLE_API_KEY_PATH in the environment)
#
# It never installs anything: `installer` needs root, so the checks read the package instead
# (signature, payload, Bom ownership, and the component's relocate and install-location settings).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP=/Applications/Cmdr.app
OUT=""
IDENTITY=""
KEYCHAIN=""
NOTARIZE=false
MIN_MACOS=10.15

while [ $# -gt 0 ]; do
    case "$1" in
        --app) APP="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        --sign) IDENTITY="$2"; shift 2 ;;
        --keychain) KEYCHAIN="$2"; shift 2 ;;
        --notarize) NOTARIZE=true; shift ;;
        -h | --help) sed -n '2,19p' "$0"; exit 0 ;;
        *) echo "Unknown argument: $1" >&2; exit 2 ;;
    esac
done

fail() { echo "build-pkg: $*" >&2; exit 1; }

[ -d "$APP/Contents" ] || fail "$APP isn't an app bundle."
if [ "$NOTARIZE" = true ] && [ -z "$IDENTITY" ]; then
    fail "--notarize needs --sign: Apple only notarizes a signed package."
fi

# The app's own identity must be the one the release config ships, so a package never wraps a
# dev build or another app by accident.
EXPECTED_ID=$(jq -r .identifier "$ROOT/apps/desktop/src-tauri/tauri.conf.json")
BUNDLE_ID=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Contents/Info.plist")
VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")
[ "$BUNDLE_ID" = "$EXPECTED_ID" ] || fail "$APP is $BUNDLE_ID, expected $EXPECTED_ID."
codesign --verify --deep --strict "$APP" || fail "$APP doesn't pass codesign --verify --deep --strict."

# File names follow the DMGs (`docs/guides/releasing.md` § What a release publishes): `x64`,
# never `x86_64`. The distribution's host architectures follow the binary, so a single-arch
# package refuses to install on the other kind of Mac instead of installing an app that can't run.
ARCHS=$(lipo -archs "$APP/Contents/MacOS/Cmdr")
case "$ARCHS" in
    "x86_64 arm64" | "arm64 x86_64") ARCH_NAME=universal; HOST_ARCHS="arm64,x86_64" ;;
    arm64) ARCH_NAME=aarch64; HOST_ARCHS=arm64 ;;
    x86_64) ARCH_NAME=x64; HOST_ARCHS=x86_64 ;;
    *) fail "unexpected architectures in the app binary: $ARCHS" ;;
esac

WORK=$(mktemp -d -t cmdr-pkg)
trap 'rm -rf "$WORK"' EXIT
[ -n "$OUT" ] || OUT=$(mktemp -d -t cmdr-pkg-out)
mkdir -p "$OUT"
PKG="$OUT/Cmdr_${VERSION}_${ARCH_NAME}.pkg"

# `ditto` keeps the signature's extended attributes and symlinks intact.
mkdir -p "$WORK/root"
ditto "$APP" "$WORK/root/Cmdr.app"

# The component plist pins how Installer treats the bundle:
# - Not relocatable: otherwise Installer looks for an app with the same bundle id anywhere on the
#   disk (a copy in Downloads, a dev build) and "upgrades" that one instead of /Applications.
# - Not version-checked: an admin can roll back by pushing an older package.
# - Upgrade in place, and match on the bundle id only.
pkgbuild --analyze --root "$WORK/root" "$WORK/component.plist" > /dev/null
COUNT=$(plutil -convert json -o - "$WORK/component.plist" | jq length)
[ "$COUNT" = 1 ] || fail "expected one bundle in the component plist, found $COUNT."
plutil -replace 0.BundleIsRelocatable -bool NO "$WORK/component.plist"
plutil -replace 0.BundleIsVersionChecked -bool NO "$WORK/component.plist"
plutil -replace 0.BundleHasStrictIdentifier -bool YES "$WORK/component.plist"
plutil -replace 0.BundleOverwriteAction -string upgrade "$WORK/component.plist"

# `--ownership recommended` (the default, stated) installs the app as root:wheel, the norm for
# /Applications. What that means for the in-app updater: the guide section above.
pkgbuild --root "$WORK/root" \
    --component-plist "$WORK/component.plist" \
    --identifier "$EXPECTED_ID" \
    --version "$VERSION" \
    --install-location /Applications \
    --ownership recommended \
    "$WORK/Cmdr-component.pkg" > /dev/null

# No scripts, no choices to customize, the system domain only, and the same macOS floor as the
# app (`minimumSystemVersion` in tauri.conf.json).
cat > "$WORK/distribution.xml" << EOF
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>Cmdr</title>
    <options customize="never" require-scripts="false" hostArchitectures="$HOST_ARCHS"/>
    <domains enable_localSystem="true" enable_currentUserHome="false" enable_anywhere="false"/>
    <volume-check>
        <allowed-os-versions>
            <os-version min="$MIN_MACOS"/>
        </allowed-os-versions>
    </volume-check>
    <choices-outline>
        <line choice="default">
            <line choice="$EXPECTED_ID"/>
        </line>
    </choices-outline>
    <choice id="default"/>
    <choice id="$EXPECTED_ID" visible="false">
        <pkg-ref id="$EXPECTED_ID"/>
    </choice>
    <pkg-ref id="$EXPECTED_ID" version="$VERSION" onConclusion="none">Cmdr-component.pkg</pkg-ref>
</installer-gui-script>
EOF

SIGN_ARGS=()
if [ -n "$IDENTITY" ]; then
    SIGN_ARGS=(--sign "$IDENTITY" --timestamp)
    [ -z "$KEYCHAIN" ] || SIGN_ARGS+=(--keychain "$KEYCHAIN")
fi
rm -f "$PKG"
productbuild --distribution "$WORK/distribution.xml" --package-path "$WORK" \
    ${SIGN_ARGS[@]+"${SIGN_ARGS[@]}"} "$PKG" > /dev/null

if [ "$NOTARIZE" = true ]; then
    : "${APPLE_API_KEY:?}" "${APPLE_API_ISSUER:?}" "${APPLE_API_KEY_PATH:?}"
    # `--wait` exits 0 whatever Apple decided, so the decision is read from the JSON status.
    RESULT=$(xcrun notarytool submit "$PKG" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" \
        --issuer "$APPLE_API_ISSUER" --wait --timeout 30m --output-format json)
    STATUS=$(jq -r .status <<< "$RESULT")
    if [ "$STATUS" != Accepted ]; then
        xcrun notarytool log "$(jq -r .id <<< "$RESULT")" --key "$APPLE_API_KEY_PATH" \
            --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" || true
        fail "notarization ended $STATUS."
    fi
    xcrun stapler staple "$PKG"
    xcrun stapler validate "$PKG"
fi

# The checks. Each reads the package as built, so a pipeline change that breaks one fails here.
EXPANDED="$WORK/expanded"
pkgutil --expand "$PKG" "$EXPANDED"
INFO="$EXPANDED/Cmdr-component.pkg/PackageInfo"
grep -q 'install-location="/Applications"' "$INFO" || fail "the component doesn't install to /Applications."
grep -q 'relocatable="false"' "$INFO" || fail "the component is relocatable."
# Every payload entry is owned by root:wheel (uid 0, gid 0).
if lsbom -p ug "$EXPANDED/Cmdr-component.pkg/Bom" | grep -qv $'^0\t0$'; then
    fail "the payload has entries not owned by root:wheel."
fi
lsbom -s "$EXPANDED/Cmdr-component.pkg/Bom" | grep -qx './Cmdr.app/Contents/MacOS/Cmdr' \
    || fail "the payload has no ./Cmdr.app/Contents/MacOS/Cmdr."
# pkgbuild stores extended attributes as `._` AppleDouble entries, which Installer turns back into
# attributes. The only one seen is `com.apple.provenance`, which macOS stamps on every file a
# process from a downloaded app writes (an agent's shell, for one) and which nothing can strip.
# Harmless (no signature or quarantine rides on it), so it warns rather than fails.
APPLEDOUBLE=$(lsbom -s "$EXPANDED/Cmdr-component.pkg/Bom" | grep -c '/\._' || true)
if [ "$APPLEDOUBLE" -gt 0 ]; then
    echo "Warning: the payload carries extended attributes as $APPLEDOUBLE \`._\` entries (see xattr -lr on the app copy)." >&2
fi

if [ -n "$IDENTITY" ]; then
    pkgutil --check-signature "$PKG"
    if [ "$NOTARIZE" = true ]; then
        spctl --assess --type install --verbose=2 "$PKG"
    fi
else
    echo "Unsigned: built without --sign."
fi

echo "Payload: $(lsbom -s "$EXPANDED/Cmdr-component.pkg/Bom" | wc -l | tr -d ' ') entries, installs to /Applications as root:wheel."
echo "$PKG"
