package main

import (
	"regexp"
	"strconv"
	"strings"
	"time"
)

// ── Cumulative CPU time ──────────────────────────────────────────────

// parseCPUTime reads one `ps -o time=` / `-o utime=` / `-o stime=` field.
//
// macOS prints `[dd-][hh:]mm:ss.ss`, and the minutes field is NOT wrapped at 60
// when there are no hours: a long-lived process reads `777:24.26`, meaning 777
// minutes. Getting that wrong silently divides a CPU number by 60.
func parseCPUTime(field string) (time.Duration, bool) {
	field = strings.TrimSpace(field)
	if field == "" {
		return 0, false
	}
	var days float64
	if dash := strings.Index(field, "-"); dash >= 0 {
		d, err := strconv.ParseFloat(field[:dash], 64)
		if err != nil {
			return 0, false
		}
		days = d
		field = field[dash+1:]
	}
	parts := strings.Split(field, ":")
	if len(parts) < 2 || len(parts) > 3 {
		return 0, false
	}
	var total float64
	for _, p := range parts {
		v, err := strconv.ParseFloat(p, 64)
		if err != nil {
			return 0, false
		}
		total = total*60 + v
	}
	total += days * 86400
	return time.Duration(total * float64(time.Second)), true
}

// ── footprint(1) ─────────────────────────────────────────────────────

// Footprint is the honest memory reading for a Cmdr process.
//
// Read `PhysBytes`, not a region total from `vmmap`: mimalloc's arenas are tagged
// `IOAccelerator` there, so a naive read attributes the Rust heap to the GPU.
// `IOAcceleratorBytes` is captured precisely so that share stays visible instead
// of being a trap the next person rediscovers.
type Footprint struct {
	PhysBytes          int64 `json:"phys_footprint_bytes"`
	PeakBytes          int64 `json:"phys_footprint_peak_bytes"`
	IOAcceleratorBytes int64 `json:"ioaccelerator_bytes"`
}

var footprintAux = regexp.MustCompile(`^\s*(phys_footprint|phys_footprint_peak):\s+(.+?)\s*$`)

// parseFootprint reads `footprint -p <pid>` output.
func parseFootprint(out string) Footprint {
	var f Footprint
	for _, line := range strings.Split(out, "\n") {
		if m := footprintAux.FindStringSubmatch(line); m != nil {
			v, ok := parseByteSize(m[2])
			if !ok {
				continue
			}
			if m[1] == "phys_footprint" {
				f.PhysBytes = v
			} else {
				f.PeakBytes = v
			}
			continue
		}
		// Region rows are `<dirty> <clean> <reclaimable> <regions> <name>`; we want
		// the dirty column of the IOAccelerator row.
		if strings.HasSuffix(strings.TrimSpace(line), "IOAccelerator") {
			if fields := strings.Fields(line); len(fields) >= 2 {
				if v, ok := parseByteSize(fields[0] + " " + fields[1]); ok {
					f.IOAcceleratorBytes = v
				}
			}
		}
	}
	return f
}

// parseByteSize reads footprint's `1507 MB` / `96 KB` / `0 B` style sizes. The
// tool uses power-of-two units despite the SI-looking suffixes.
func parseByteSize(s string) (int64, bool) {
	fields := strings.Fields(s)
	if len(fields) != 2 {
		return 0, false
	}
	v, err := strconv.ParseFloat(fields[0], 64)
	if err != nil {
		return 0, false
	}
	mult := map[string]float64{"B": 1, "KB": 1 << 10, "MB": 1 << 20, "GB": 1 << 30, "TB": 1 << 40}[fields[1]]
	if mult == 0 {
		return 0, false
	}
	return int64(v * mult), true
}

// ── Log lines ────────────────────────────────────────────────────────

// RowWrites counts what a rebuild actually wrote, summed over a phase. These come
// from the app's own reconcile summaries, which is the only place the numbers are
// per-rebuild rather than per-process.
type RowWrites struct {
	Rebuilds int   `json:"rebuilds"`
	Added    int64 `json:"rows_added"`
	Removed  int64 `json:"rows_removed"`
	Updated  int64 `json:"rows_updated"`
	Millis   int64 `json:"rebuild_millis_total"`
}

// Total is the row count the write path paid for: every insert, delete, and update
// is one row through the writer.
func (r RowWrites) Total() int64 { return r.Added + r.Removed + r.Updated }

var (
	// `MustScanSubDirs: reconcile complete for <path> (+8 -0 ~0, 0ms)`
	subtreeRebuild = regexp.MustCompile(`reconcile complete for (.+) \(\+(\d+) -(\d+) ~(\d+), (\d+)ms\)`)
	// `local reconcile: complete for <path>: +8 -0 ~0 (3 dirs re-listed) in 12ms`
	localRebuild = regexp.MustCompile(`local reconcile: complete for (.+): \+(\d+) -(\d+) ~(\d+) .* in (\d+)ms`)
)

// Rebuild is one directory rebuild as the app reported it. The path is kept
// because this machine runs several build trees at once: rows written elsewhere
// are somebody else's churn, and folding them in would inflate the baseline.
type Rebuild struct {
	Path                    string
	Added, Removed, Updated int64
	Millis                  int64
}

// parseRebuildLine reads a rebuild summary out of a log line.
func parseRebuildLine(line string) (Rebuild, bool) {
	m := subtreeRebuild.FindStringSubmatch(line)
	if m == nil {
		m = localRebuild.FindStringSubmatch(line)
	}
	if m == nil {
		return Rebuild{}, false
	}
	return Rebuild{
		Path:    m[1],
		Added:   atoi(m[2]),
		Removed: atoi(m[3]),
		Updated: atoi(m[4]),
		Millis:  atoi(m[5]),
	}, true
}

// `ComputePartialAggregates(Sql): 12 dirs computed, 34 rows written, 2/3 hot paths resolved (5ms)`
var aggregateWrite = regexp.MustCompile(`ComputePartialAggregates\([^)]*\): (\d+) dirs computed, (\d+) rows written`)

// AggregateWrites counts `dir_stats` rows the aggregator wrote. Kept apart from
// RowWrites because these are a different table and a different cost: delta
// propagation into `dir_stats`, not per-file `entries` rows. Summing the two would
// hide which of the two moved.
type AggregateWrites struct {
	Passes int   `json:"passes"`
	Dirs   int64 `json:"dirs_computed"`
	Rows   int64 `json:"rows_written"`
}

func parseAggregateLine(line string) (AggregateWrites, bool) {
	m := aggregateWrite.FindStringSubmatch(line)
	if m == nil {
		return AggregateWrites{}, false
	}
	return AggregateWrites{Passes: 1, Dirs: atoi(m[1]), Rows: atoi(m[2])}, true
}

func (a *AggregateWrites) add(o AggregateWrites) {
	a.Passes += o.Passes
	a.Dirs += o.Dirs
	a.Rows += o.Rows
}

func (a AggregateWrites) sub(o AggregateWrites) AggregateWrites {
	return AggregateWrites{Passes: a.Passes - o.Passes, Dirs: a.Dirs - o.Dirs, Rows: a.Rows - o.Rows}
}

// `Writer: +271 msgs (242 upserts, 5 deletes, 7 delete_subtrees, 7 flushes, 10 others) in 5.0s [383707 total]`
var writerRollup = regexp.MustCompile(`Writer: \+\d+ msgs?(?: \(([^)]*)\))? in [\d.]+s \[(\d+) total\]`)

// WriterRollup is the AUTHORITATIVE count of what went through the write path.
//
// It reads the writer's own rolling counter rather than inferring row writes from
// reconcile summaries: those cover one path only, and a first measurement showed
// they account for a small fraction of what the writer actually processes. Total
// is cumulative since the process started, so a phase diffs it and cannot
// undercount because a log line was missed. ByKind sums the per-window deltas the
// same lines break out, which is where "how many of these were upserts" comes
// from.
type WriterRollup struct {
	Total  int64            `json:"messages_total_cumulative"`
	ByKind map[string]int64 `json:"messages_by_kind"`
}

func newWriterRollup() WriterRollup { return WriterRollup{ByKind: map[string]int64{}} }

func parseWriterRollup(line string) (breakdown string, total int64, ok bool) {
	m := writerRollup.FindStringSubmatch(line)
	if m == nil {
		return "", 0, false
	}
	return m[1], atoi(m[2]), true
}

// addLine folds one writer rollup line in, returning true when it matched.
func (w *WriterRollup) addLine(line string) bool {
	breakdown, total, ok := parseWriterRollup(line)
	if !ok {
		return false
	}
	w.Total = total
	if w.ByKind == nil {
		w.ByKind = map[string]int64{}
	}
	for _, part := range strings.Split(breakdown, ", ") {
		count, kind, found := strings.Cut(strings.TrimSpace(part), " ")
		if !found {
			continue
		}
		n, err := strconv.ParseInt(count, 10, 64)
		if err != nil {
			continue
		}
		// The writer pluralizes per line, so `1 upsert` and `2 upserts` are the
		// same kind. Fold them, or the map grows a duplicate key per singular.
		w.ByKind[singularize(kind)] += n
	}
	return true
}

// singularize undoes the writer's pluralization so both forms fold to one key.
// Naively trimming a trailing "s" turns `flushes` into `flushe` and reports the
// same counter under two names, which matters because this JSON is a
// before/after contract.
func singularize(word string) string {
	for _, ending := range []string{"shes", "ches", "sses", "xes", "zes"} {
		if strings.HasSuffix(word, ending) {
			return strings.TrimSuffix(word, "es")
		}
	}
	return strings.TrimSuffix(word, "s")
}

func (w WriterRollup) sub(o WriterRollup) WriterRollup {
	out := WriterRollup{Total: w.Total - o.Total, ByKind: map[string]int64{}}
	for kind, n := range w.ByKind {
		if d := n - o.ByKind[kind]; d != 0 {
			out.ByKind[kind] = d
		}
	}
	return out
}

// `MustScanSubDirs: shallow anchor / inside the sweep window; coalescing (220 since the last sweep)`
var coalescedAnchor = regexp.MustCompile(`shallow anchor .* coalescing \(\d+ since the last sweep\)`)

// `Reconciler: skipped 584 removals for unknown paths in 60.2s [19073 total], sample: …`
var skippedEvents = regexp.MustCompile(`Reconciler: skipped (\d+) (.+?) in [\d.]+s \[\d+ total\]`)

// EventWork counts what the reconciler did with events it did NOT turn into row
// writes. It is here because a first measurement showed this dominates: in a
// 20-minute build loop the app coalesced far more anchors than it reconciled, and
// discarded events by the thousand. A baseline that counted only rows written
// would report almost nothing and miss where the CPU actually goes.
type EventWork struct {
	CoalescedAnchors int              `json:"coalesced_shallow_anchors"`
	SkippedEvents    int64            `json:"skipped_events"`
	SkippedByReason  map[string]int64 `json:"skipped_events_by_reason"`
}

func newEventWork() EventWork { return EventWork{SkippedByReason: map[string]int64{}} }

// addLine folds a coalescing or skip line in, returning true when it matched.
func (e *EventWork) addLine(line string) bool {
	if coalescedAnchor.MatchString(line) {
		e.CoalescedAnchors++
		return true
	}
	m := skippedEvents.FindStringSubmatch(line)
	if m == nil {
		return false
	}
	n := atoi(m[1])
	e.SkippedEvents += n
	if e.SkippedByReason == nil {
		e.SkippedByReason = map[string]int64{}
	}
	e.SkippedByReason[m[2]] += n
	return true
}

func (e EventWork) sub(o EventWork) EventWork {
	out := EventWork{
		CoalescedAnchors: e.CoalescedAnchors - o.CoalescedAnchors,
		SkippedEvents:    e.SkippedEvents - o.SkippedEvents,
		SkippedByReason:  map[string]int64{},
	}
	for reason, n := range e.SkippedByReason {
		if d := n - o.SkippedByReason[reason]; d != 0 {
			out.SkippedByReason[reason] = d
		}
	}
	return out
}

func (r *RowWrites) add(b Rebuild) {
	r.Rebuilds++
	r.Added += b.Added
	r.Removed += b.Removed
	r.Updated += b.Updated
	r.Millis += b.Millis
}

func (r RowWrites) sub(o RowWrites) RowWrites {
	return RowWrites{
		Rebuilds: r.Rebuilds - o.Rebuilds,
		Added:    r.Added - o.Added,
		Removed:  r.Removed - o.Removed,
		Updated:  r.Updated - o.Updated,
		Millis:   r.Millis - o.Millis,
	}
}

func atoi(s string) int64 {
	v, _ := strconv.ParseInt(s, 10, 64)
	return v
}

// `heartbeat volume_id=root queue_depth=0 … time_in_commit_ms=3 writer_cpu_ms_total=41231`
var writerHeartbeat = regexp.MustCompile(`heartbeat volume_id=(\S+) .*writer_cpu_ms_total=(\d+)`)

// WriterCPU is the index writer THREAD's cumulative CPU time, per volume.
//
// It exists because the whole-process CPU number this harness reports is too
// coarse for any claim about the index write path. Under 20 minutes of real build
// churn the app spent ~130 CPU-seconds, of which the per-row upserts were on the
// order of 1%: far under the noise on a machine where several build trees churn at
// once. Arguing that from `app_cpu_seconds` is hopeless, and macOS gives no help
// from outside, since `ps -M` reports per-thread cumulative CPU but no thread
// names. The app therefore exports its own counter and this reads it.
//
// PER VOLUME because each volume runs its own writer thread with its own counter;
// folding them into one number would make a NAS reconnect look like write-path
// work on the boot disk. `root` is the local index, which is the one the
// before/after comparison is about.
//
// Cumulative, so a phase is the difference of two readings — never a sampled
// rate, for the same reason the app-CPU numbers are integrated rather than
// sampled (§ "CPU and memory under churn" in the baseline note).
type WriterCPU struct {
	MsByVolume map[string]int64 `json:"writer_cpu_ms_by_volume"`
}

func newWriterCPU() WriterCPU { return WriterCPU{MsByVolume: map[string]int64{}} }

// addLine folds one writer heartbeat in, returning true when it matched.
func (w *WriterCPU) addLine(line string) bool {
	m := writerHeartbeat.FindStringSubmatch(line)
	if m == nil {
		return false
	}
	if w.MsByVolume == nil {
		w.MsByVolume = map[string]int64{}
	}
	// Last value wins: the counter is cumulative, so the newest reading is the
	// running total for that volume's writer thread.
	w.MsByVolume[m[1]] = atoi(m[2])
	return true
}

// RootMs is the local index writer's counter, the one the comparison is about.
func (w WriterCPU) RootMs() int64 { return w.MsByVolume["root"] }

func (w WriterCPU) sub(o WriterCPU) WriterCPU {
	out := WriterCPU{MsByVolume: map[string]int64{}}
	for volume, ms := range w.MsByVolume {
		// A volume with no BEFORE reading would report its whole since-launch
		// total as this phase's cost, which is wildly wrong for a phase that ran
		// minutes into the session. Report only volumes seen on both ends; the
		// settle phase exists to give every live writer a first reading.
		if before, seen := o.MsByVolume[volume]; seen {
			out.MsByVolume[volume] = ms - before
		}
	}
	return out
}
