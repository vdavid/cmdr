#!/usr/bin/env python3
"""How many samples a `sample` window caught inside the listing's visibility scan.

The wedge's leaf frames are the visibility predicate and the substring test
inside it (`cmdr_fs::staging::is_staging_temp_name`), which `sample` reports in
its "Sort by top of stack" histogram already demangled. Counting them is the
sharpest before/after signal: the share of the main thread that is answering IPC
saturates near 100% under any load, but the number of samples spent WALKING is
the cost the row map removes.

Usage: listing-scan-leaves.py <sample-output-file>
"""

import re
import sys

SCAN_LEAVES = ("visible_entries", "simd_contains", "<str>::contains", "is_contained_in")


def main() -> None:
    text = open(sys.argv[1]).read()
    histogram = text[text.index("Sort by top of stack") :]
    histogram = histogram[: histogram.index("Binary Images")]

    scan = 0
    for line in histogram.splitlines()[1:]:
        match = re.match(r"^\s+(.*?)\s+\(in \S+\)\s+(\d+)\s*$", line)
        if match and any(leaf in match.group(1) for leaf in SCAN_LEAVES):
            scan += int(match.group(2))

    print(f"{scan} samples with a leaf inside the listing visibility scan")


if __name__ == "__main__":
    main()
