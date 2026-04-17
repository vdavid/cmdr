#!/bin/sh
# Create files with very long names and a deep path for MAX_PATH testing.
BASE="/shares/public"
mkdir -p "$BASE"

# 220-character filename (well over the classic 255 limit territory)
LONG_NAME=$(printf 'a%.0s' $(seq 1 220))
echo "Long filename content" > "$BASE/${LONG_NAME}.txt"

# Another long name with mixed characters
LONG_NAME2=$(printf 'document_with_a_very_long_name_that_tests_path_limits_%.0s' $(seq 1 4))
echo "Another long filename" > "$BASE/${LONG_NAME2}.txt"

# Deep path to test MAX_PATH-adjacent scenarios (total path > 260 chars)
DEEP="$BASE"
for i in $(seq 1 10); do
    DEEP="$DEEP/level_$(printf '%02d' "$i")_with_padding"
done
mkdir -p "$DEEP"
echo "Deep file content" > "$DEEP/deep_file.txt"

# A normal file for comparison
echo "Normal filename for reference" > "$BASE/normal.txt"

chmod -R 777 "$BASE"
