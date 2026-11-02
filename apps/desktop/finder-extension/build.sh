#!/bin/sh
# Builds the Finder extension in two steps, split the way the release pipeline is split: the
# `compile` job compiles, and the `sign` job assembles and signs without compiling anything.
#
#   build.sh compile <rust-target> <out-binary>
#   build.sh assemble <out.appex> <version> <bundle-id> <signing-identity> <binary>...
#
# More than one binary means a universal extension, joined with `lipo`. A signing identity of `-`
# signs ad hoc, for a local try-out. In CI, `$LLAMA_SIGN_KEYCHAIN` names the keychain to sign from.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"

case "${1:-}" in
compile)
  rust_target="$2"
  out="$3"
  case "$rust_target" in
    aarch64-apple-darwin) swift_target=arm64-apple-macos11 ;;
    x86_64-apple-darwin) swift_target=x86_64-apple-macos10.15 ;;
    *)
      echo "Unknown target: $rust_target" >&2
      exit 2
      ;;
  esac
  mkdir -p "$(dirname "$out")"
  # `_NSExtensionMain` is the entry point every app extension has.
  swiftc -O -whole-module-optimization -target "$swift_target" \
    -module-name CmdrFinderExtension -parse-as-library -application-extension \
    -Xlinker -e -Xlinker _NSExtensionMain -framework FinderSync \
    -o "$out" "$here/OpenInCmdr.swift"
  ;;
assemble)
  appex="$2"
  version="$3"
  bundle_id="$4"
  identity="$5"
  shift 5
  rm -rf "$appex"
  mkdir -p "$appex/Contents/MacOS"
  lipo -create -output "$appex/Contents/MacOS/CmdrFinderExtension" "$@"
  chmod 755 "$appex/Contents/MacOS/CmdrFinderExtension"
  plist="$appex/Contents/Info.plist"
  cp "$here/Info.plist" "$plist"
  /usr/libexec/PlistBuddy \
    -c "Set :CFBundleIdentifier $bundle_id" \
    -c "Set :CFBundleVersion $version" \
    -c "Set :CFBundleShortVersionString $version" "$plist"
  set -- --force --sign "$identity" --entitlements "$here/Entitlements.plist"
  if [ "$identity" != "-" ]; then
    # Notarization refuses a nested binary without a timestamp and the hardened runtime.
    set -- "$@" --options runtime --timestamp
  fi
  if [ -n "${LLAMA_SIGN_KEYCHAIN:-}" ]; then
    set -- "$@" --keychain "$LLAMA_SIGN_KEYCHAIN"
  fi
  codesign "$@" "$appex"
  ;;
*)
  echo "Usage: build.sh compile|assemble ..." >&2
  exit 2
  ;;
esac
