#!/usr/bin/env bash
# Safe FAT32 probe on a SYNTHETIC disk image (never a physical card).
# Every hdiutil/diskutil call is timeout-guarded so a wedged FSKit msdos
# service can't sit in an uninterruptible hang and cascade into a panic.
# Single detach at teardown via an EXIT trap; no mount/unmount cycling.
set -uo pipefail

TO() { timeout --signal=KILL 30 "$@"; }   # hard 30s cap, SIGKILL so nothing lingers

WORK="$(mktemp -d /tmp/fat32-probe.XXXXXX)"
IMG="$WORK/fat32.dmg"
DEV=""
MNT=""

cleanup() {
  echo; echo "=== TEARDOWN ==="
  if [ -n "$DEV" ]; then
    echo "detaching $DEV ..."
    if TO hdiutil detach "$DEV" >/dev/null 2>&1; then
      echo "detached cleanly"
    else
      echo "clean detach failed/timed out -> forcing"
      TO hdiutil detach -force "$DEV" >/dev/null 2>&1 && echo "force-detached" || echo "FORCE DETACH ALSO FAILED (dev $DEV may still be attached)"
    fi
  fi
  rm -rf "$WORK" 2>/dev/null
  echo "removed $WORK"
}
trap cleanup EXIT

echo "=== CREATE 100MB FAT32 image ==="
TO hdiutil create -size 100m -fs "MS-DOS FAT32" -volname CMDRTEST -layout MBRSPUD "$IMG" || { echo "create failed"; exit 1; }

echo; echo "=== ATTACH (nobrowse) ==="
ATTACH_OUT="$(TO hdiutil attach "$IMG" -nobrowse 2>&1)" || { echo "attach failed: $ATTACH_OUT"; exit 1; }
echo "$ATTACH_OUT"
DEV="$(echo "$ATTACH_OUT" | awk '/FDisk_partition_scheme/{print $1; exit}')"
# The FAT partition is the mounted one:
MNT="$(echo "$ATTACH_OUT" | awk -F'\t' '/\/Volumes\//{print $NF; exit}')"
[ -z "$DEV" ] && DEV="$(echo "$ATTACH_OUT" | awk 'NR==1{print $1}')"
echo "DEV=$DEV  MNT=$MNT"
[ -z "$MNT" ] && { echo "no mount point parsed"; exit 1; }

echo; echo "=== diskutil info (fs personality, UUID, read-only) ==="
TO diskutil info "$MNT" | grep -iE "File System Personality|Volume Name|Volume UUID|Read-Only|Partition Type|Media Type" || true

echo; echo "=== statfs read-only flag (mount opts) ==="
mount | grep -F "$MNT" || true

echo; echo "=== INODE PROBES ==="
cd "$MNT"
echo "-- create a file with content --"
printf 'hello world' > probe_a.txt
stat -f 'A after-create: ino=%i nlink=%l size=%z name=%N' probe_a.txt

echo "-- same file, inode across 3 SEPARATE processes (the decisive test) --"
for i in 1 2 3; do
  bash -c "stat -f 'A proc$i: ino=%i' '$MNT/probe_a.txt'"
done

echo "-- empty file inode, then after adding content --"
: > probe_empty.txt
stat -f 'empty: ino=%i size=%z' probe_empty.txt
printf 'now has content' > probe_empty.txt
stat -f 'filled: ino=%i size=%z' probe_empty.txt

echo "-- inode across rename (same dir) --"
stat -f 'before-rename: ino=%i' probe_a.txt
mv probe_a.txt probe_a_renamed.txt
stat -f 'after-rename:  ino=%i' probe_a_renamed.txt

echo "-- inode across move to subdir --"
mkdir -p sub
mv probe_a_renamed.txt sub/probe_a_moved.txt
stat -f 'after-move:    ino=%i' sub/probe_a_moved.txt

echo "-- delete + create: inode reuse? --"
stat -f 'X before-del: ino=%i' probe_empty.txt
rm probe_empty.txt
printf 'brand new file' > probe_y.txt
stat -f 'Y after-new:  ino=%i' probe_y.txt

echo "-- nlink for a directory --"
stat -f 'dir sub: ino=%i nlink=%l' sub

echo; echo "=== FSEVENTS journal availability ==="
ls -la "$MNT/.fseventsd" 2>&1 | head -20 || echo "no .fseventsd"
if [ -f "$MNT/.fseventsd/no_log" ]; then echo ">> no_log present: FSEvents REPLAY is NOT available for this volume"; fi

cd /
echo; echo "=== done probing ==="
