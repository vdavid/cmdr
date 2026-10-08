#!/usr/bin/env bash
# Take one memory reading of a running Cmdr and append it to a CSV, so an A/B over hours
# is one command per sample instead of a remembered `vmmap` incantation.
#
# The numbers that matter and the traps around them: `docs/tooling/memory-debugging.md`.
# The short version: the `rustHeap*` columns read VM tag 100, which `vmmap` prints as
# `IOAccelerator`. In a mimalloc build (Linux, or macOS with `--features mimalloc`) that's
# where mimalloc's arenas are, so those columns ARE the Rust heap and the `MALLOC_*` ones
# are NOT. A default macOS build runs on the system allocator, which flips it: the Rust
# heap is in the `MALLOC_*` columns and `rustHeap*` reads ~0. Footprint counts swapped
# pages, so dirty + swapped is what a user feels.
#
# Usage:
#   mem-sample.sh <condition-label>        one sample, appended to the CSV
#   mem-sample.sh --watch <label> [mins]   sample every N minutes (default 15) until Ctrl-C
#   mem-sample.sh --show                   print the CSV so far
#   mem-sample.sh --env                    print the mimalloc options the next launch will see
#
# CSV lands in $CMDR_MEM_CSV, default ~/cmdr-mem-experiment.csv.

set -euo pipefail

CSV="${CMDR_MEM_CSV:-$HOME/cmdr-mem-experiment.csv}"

# Every mimalloc option this experiment touches. `launchctl getenv` is the only way to see
# what a Finder-launched app inherited, since `ps -E` on another process needs root.
MI_OPTIONS=(
	MIMALLOC_PURGE_DELAY
	MIMALLOC_ARENA_PURGE_MULT
	MIMALLOC_PURGE_DECOMMITS
	MIMALLOC_ARENA_EAGER_COMMIT
	MIMALLOC_ARENA_RESERVE
	MIMALLOC_PAGE_COMMIT_ON_DEMAND
	MIMALLOC_PAGE_FULL_RETAIN
	MIMALLOC_PAGE_RECLAIM_ON_FREE
	MIMALLOC_GENERIC_COLLECT
	MIMALLOC_SHOW_STATS
)

die() {
	echo "mem-sample: $*" >&2
	exit 1
}

# vmmap prints sizes as a bare number with a K/M/G/T suffix, where M means MiB. Turn one
# into whole MiB, so nothing here ever mixes binary and decimal units.
to_mb() {
	awk -v s="$1" 'BEGIN {
		n = s + 0
		u = substr(s, length(s), 1)
		if (u == "K") n /= 1024
		else if (u == "G") n *= 1024
		else if (u == "T") n *= 1048576
		else if (u ~ /[0-9]/) n /= 1048576   # bare bytes
		printf "%.0f", n
	}'
}

print_env() {
	local name value any=0
	for name in "${MI_OPTIONS[@]}"; do
		value="$(launchctl getenv "$name" || true)"
		if [[ -n $value ]]; then
			echo "  $name=$value"
			any=1
		fi
	done
	[[ $any -eq 1 ]] || echo "  (none set — mimalloc defaults)"
}

# The env string that goes in the CSV, so a row can never be misfiled under the wrong
# condition: it records what was actually set, not what we meant to set.
env_summary() {
	local name value parts=()
	for name in "${MI_OPTIONS[@]}"; do
		value="$(launchctl getenv "$name" || true)"
		[[ -n $value ]] && parts+=("${name#MIMALLOC_}=$value")
	done
	if [[ ${#parts[@]} -eq 0 ]]; then
		echo "defaults"
	else
		(
			IFS=' '
			echo "${parts[*]}"
		)
	fi
}

sample() {
	local label="$1" pid out
	# The installed app, deliberately: a `cargo run` release build is a different binary
	# with a different data dir, and sampling it by accident silently ruins a condition.
	# CMDR_MEM_PID overrides when you really do mean another instance.
	pid="${CMDR_MEM_PID:-$(pgrep -f '^/Applications/Cmdr\.app/Contents/MacOS/Cmdr$' | head -1)}"
	[[ -n $pid ]] || die "the /Applications build of Cmdr isn't running (set CMDR_MEM_PID to sample another instance)"

	out="$(vmmap -summary "$pid" 2>/dev/null)" || die "vmmap failed for pid $pid"
	[[ -n $out ]] || die "vmmap returned nothing for pid $pid (is it still alive?)"

	local footprint peak uptime_min
	footprint="$(to_mb "$(awk '/^Physical footprint:/ {print $NF}' <<<"$out")")"
	peak="$(to_mb "$(awk '/^Physical footprint \(peak\):/ {print $NF}' <<<"$out")")"

	# Uptime matters because the fresh-launch rule makes every reading a function of it:
	# a 20-minute sample and a 20-hour sample are not the same measurement.
	# BSD `ps` has no `etimes`, only `etime` as `[[dd-]hh:]mm:ss`.
	uptime_min="$(ps -o etime= -p "$pid" | awk '{
		gsub(/^ +| +$/, "", $0)
		split($0, a, "-")
		d = (2 in a) ? a[1] : 0
		n = split((2 in a) ? a[2] : a[1], t, ":")
		s = (n == 3) ? t[1]*3600 + t[2]*60 + t[3] : t[1]*60 + t[2]
		printf "%d", (d*86400 + s) / 60
	}')"

	# Only the region-type table. Everything below the `MALLOC ZONE` header is a per-zone
	# table with a different column layout, and its rows would otherwise be parsed as tags.
	local regions
	regions="$(awk '/^REGION TYPE/ {on = 1} /^MALLOC ZONE/ {exit} on' <<<"$out")"

	# A row is `<tag padded to col 32><VIRTUAL><RESIDENT><DIRTY><SWAPPED>…<COUNT>`, so tags
	# with spaces in them ("Malloc Small") shift every $N. Slice the tag by column instead.
	#
	# ⚠️ On macOS 27 the system-zone tags are `Malloc Small` / `Malloc Large`, NOT the
	# `MALLOC_SMALL` / `MALLOC_LARGE` older recipes grep for. That spelling silently matches
	# nothing and reads as zero: 65 MB of `Malloc Small` was missed this way on 2026-09-22.
	# Match on a prefix so a rename to either spelling still lands somewhere.
	sum_tag() {
		awk -v want="$1" -v col="$2" -v mode="${3:-exact}" '
			{
				tag = substr($0, 1, 32); gsub(/^ +| +$/, "", tag)
				# Trim before splitting: a leading run of spaces would otherwise make f[1]
				# empty and shift every column by one.
				rest = substr($0, 33); gsub(/^[ \t]+|[ \t]+$/, "", rest)
				n = split(rest, f, /[ \t]+/)   # 1 VIRTUAL 2 RESIDENT 3 DIRTY 4 SWAPPED … 8 COUNT
				if (n < 8) next
				if (mode == "exact" ? (tag == want) : (index(tolower(tag), tolower(want)) == 1)) {
					print f[col]
				}
			}
		' <<<"$regions"
	}

	mb_sum() {
		local total=0 v
		while read -r v; do
			[[ -n $v ]] && total=$((total + $(to_mb "$v")))
		done
		echo "$total"
	}

	local heap_dirty heap_swapped heap_regions heap_total sys_malloc malloc_large
	# `IOAccelerator` exactly: the `(reserved)` and `(graphics)` siblings are not the heap.
	heap_dirty="$(sum_tag IOAccelerator 3 | mb_sum)"
	heap_swapped="$(sum_tag IOAccelerator 4 | mb_sum)"
	heap_regions="$(sum_tag IOAccelerator 8 | awk '{t += $1} END {print t + 0}')"
	heap_total=$((heap_dirty + heap_swapped))
	# Every `Malloc *` row together (Small, Tiny, Nano, Large, metadata, and the empties):
	# the system allocator's whole footprint, which is NOT the Rust heap.
	sys_malloc=$(($(sum_tag Malloc 3 prefix | mb_sum) + $(sum_tag Malloc 4 prefix | mb_sum)))
	# Broken out because a big one is the CLIP-tower fingerprint (see memory-debugging.md).
	malloc_large=$(($(sum_tag "Malloc Large" 3 prefix | mb_sum) + $(sum_tag "Malloc Large" 4 prefix | mb_sum)))

	if [[ ! -f $CSV ]]; then
		echo "timestamp,condition,mimallocEnv,pid,uptimeMin,footprintMiB,peakFootprintMiB,rustHeapDirtyMiB,rustHeapSwappedMiB,rustHeapTotalMiB,rustHeapRegions,sysMallocMiB,mallocLargeMiB" >"$CSV"
	fi
	printf '%s,%s,"%s",%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
		"$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$label" "$(env_summary)" "$pid" "${uptime_min:-}" \
		"$footprint" "$peak" "$heap_dirty" "$heap_swapped" "$heap_total" \
		"${heap_regions:-0}" "$sys_malloc" "$malloc_large" >>"$CSV"

	echo "[$label] pid $pid, up ${uptime_min:-?} min"
	echo "  footprint ${footprint} MiB (peak ${peak} MiB)"
	echo "  Rust heap ${heap_total} MiB = ${heap_dirty} dirty + ${heap_swapped} swapped, in ${heap_regions:-0} regions"
	echo "    minus the 63 MiB SQLite slab → ~$((heap_total - 63)) MiB with no name on it"
	echo "  system malloc ${sys_malloc} MiB, BESIDE the heap (of which Malloc Large ${malloc_large} MiB)"
	echo "  mimalloc env: $(env_summary)"
	echo "  → $CSV"
}

case "${1:-}" in
"")
	die "need a condition label, e.g. 'baseline'. See --help."
	;;
-h | --help)
	sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
	;;
--env)
	echo "mimalloc options the NEXT launch will inherit:"
	print_env
	;;
--show)
	[[ -f $CSV ]] || die "no samples yet at $CSV"
	column -s, -t "$CSV"
	;;
--watch)
	label="${2:-}"
	[[ -n $label ]] || die "--watch needs a condition label"
	every=$(((${3:-15}) * 60))
	echo "Sampling '$label' every $((every / 60)) min. Ctrl-C to stop."
	while true; do
		sample "$label"
		echo
		sleep "$every"
	done
	;;
*)
	sample "$1"
	;;
esac
