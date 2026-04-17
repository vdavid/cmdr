#!/bin/sh
# Create a 50-level deep directory tree with one file at each level.
BASE="/shares/public"
CURRENT="$BASE"
for i in $(seq 1 50); do
    CURRENT="$CURRENT/level_$(printf '%02d' "$i")"
    mkdir -p "$CURRENT"
    echo "File at depth $i" > "$CURRENT/file.txt"
done

chmod -R 777 "$BASE"
