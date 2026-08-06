package main

import (
	"strings"
	"testing"
	"time"
)

func TestParseCPUTime(t *testing.T) {
	cases := []struct {
		in   string
		want time.Duration
		ok   bool
	}{
		{"0:00.01", 10 * time.Millisecond, true},
		{"6:43.55", 403*time.Second + 550*time.Millisecond, true},
		// The trap: `ps` does NOT wrap minutes at 60 when there's no hours field,
		// so this real reading from a long-lived Chrome is 777 minutes, not 12:57.
		{"777:24.26", 777*time.Minute + 24*time.Second + 260*time.Millisecond, true},
		{"1:02:03.00", time.Hour + 2*time.Minute + 3*time.Second, true},
		{"2-01:00:00.00", 49 * time.Hour, true},
		{"", 0, false},
		{"nope", 0, false},
		{"1:2:3:4", 0, false},
	}
	for _, c := range cases {
		got, ok := parseCPUTime(c.in)
		if ok != c.ok || got != c.want {
			t.Errorf("parseCPUTime(%q) = %v, %v; want %v, %v", c.in, got, ok, c.want, c.ok)
		}
	}
}

// Verbatim `footprint -p` output, trimmed to the rows that matter. The
// IOAccelerator row is real: 382 MB of it is mimalloc's arenas, not GPU memory,
// which is the whole reason this tool reads phys_footprint instead.
const footprintSample = `
 880 MB        0 B          0 B        180    MALLOC_LARGE
 382 MB        0 B       156 MB         21    IOAccelerator
 230 MB        0 B          0 B        157    MALLOC_SMALL
 864 KB        0 B          0 B         52    IOAccelerator (graphics)
    ---        ---          ---        ---    ---
1507 MB     152 MB       609 MB       9495    TOTAL

Auxiliary data:
    neural_peak: 93 MB
    phys_footprint: 1507 MB
    phys_footprint_peak: 1981 MB
`

func TestParseFootprint(t *testing.T) {
	got := parseFootprint(footprintSample)
	if want := int64(1507) << 20; got.PhysBytes != want {
		t.Errorf("PhysBytes = %d, want %d", got.PhysBytes, want)
	}
	if want := int64(1981) << 20; got.PeakBytes != want {
		t.Errorf("PeakBytes = %d, want %d", got.PeakBytes, want)
	}
	// The plain IOAccelerator row, not `IOAccelerator (graphics)`.
	if want := int64(382) << 20; got.IOAcceleratorBytes != want {
		t.Errorf("IOAcceleratorBytes = %d, want %d", got.IOAcceleratorBytes, want)
	}
}

func TestParseRebuildLine(t *testing.T) {
	// Both matching lines are verbatim from a real cmdr.log.
	lines := []string{
		`2026-08-06T01:05:58.890+02:00 DEBUG cmdr_index::indexing::reconcile::reconciler::rescan  ` +
			`MustScanSubDirs: reconcile complete for /private/tmp/x (+8 -0 ~0, 0ms)`,
		`2026-08-06T01:05:58.891+02:00 INFO  cmdr_index  ` +
			`local reconcile: complete for /Users/x/target: +12 -3 ~40 (7 dirs re-listed) in 512ms`,
		`2026-08-06T01:05:59.000+02:00 DEBUG something else entirely`,
	}
	var all, mine RowWrites
	for _, l := range lines {
		b, ok := parseRebuildLine(l)
		if !ok {
			continue
		}
		all.add(b)
		if strings.HasPrefix(b.Path, "/Users/x") {
			mine.add(b)
		}
	}
	if all.Rebuilds != 2 || all.Added != 20 || all.Removed != 3 || all.Updated != 40 || all.Millis != 512 {
		t.Errorf("all = %+v", all)
	}
	if all.Total() != 63 {
		t.Errorf("all.Total() = %d, want 63", all.Total())
	}
	// The path split is what keeps another build tree's churn out of the baseline.
	if mine.Rebuilds != 1 || mine.Total() != 55 {
		t.Errorf("mine = %+v, Total = %d; want 1 rebuild totalling 55", mine, mine.Total())
	}
	if got := all.sub(mine); got.Rebuilds != 1 || got.Total() != 8 {
		t.Errorf("all.sub(mine) = %+v, Total = %d; want 1 rebuild totalling 8", got, got.Total())
	}
}

// A path with spaces still parses: the greedy capture is anchored by the counts
// that follow it, and real paths here include `Application Support`.
func TestParseRebuildLinePathWithSpaces(t *testing.T) {
	line := `... MustScanSubDirs: reconcile complete for /Users/x/Library/Application Support/y (+3 -1 ~2, 7ms)`
	b, ok := parseRebuildLine(line)
	if !ok {
		t.Fatal("did not match")
	}
	if b.Path != "/Users/x/Library/Application Support/y" {
		t.Errorf("Path = %q", b.Path)
	}
	if b.Added != 3 || b.Removed != 1 || b.Updated != 2 || b.Millis != 7 {
		t.Errorf("got %+v", b)
	}
}

func TestUnderAny(t *testing.T) {
	roots := []string{"/a/target", "/b/.cargo/registry/"}
	cases := map[string]bool{
		"/a/target":                   true,
		"/a/target/debug/incremental": true,
		"/b/.cargo/registry/src/x":    true,
		// A sibling that merely shares a prefix must NOT count; without the
		// separator check `/a/targetish` would land inside `/a/target`.
		"/a/targetish": false,
		"/a/tar":       false,
		"/elsewhere":   false,
	}
	for path, want := range cases {
		if got := underAny(path, roots); got != want {
			t.Errorf("underAny(%q) = %v, want %v", path, got, want)
		}
	}
	if underAny("/a/target", nil) {
		t.Error("underAny with no roots should be false")
	}
}

func TestParseAggregateLine(t *testing.T) {
	// Verbatim shape from writer/aggregation.rs.
	line := `2026-08-06T07:00:00.000+02:00 INFO  cmdr_index  ` +
		`ComputePartialAggregates(Sql): 12 dirs computed, 34 rows written, 2/3 hot paths resolved (5ms)`
	got, ok := parseAggregateLine(line)
	if !ok {
		t.Fatal("did not match")
	}
	if got.Passes != 1 || got.Dirs != 12 || got.Rows != 34 {
		t.Errorf("got %+v", got)
	}
	if _, ok := parseAggregateLine("something else"); ok {
		t.Error("matched a non-aggregate line")
	}
}

func TestEventWork(t *testing.T) {
	// Both lines verbatim from a real cmdr.log.
	lines := []string{
		`... MustScanSubDirs: shallow anchor / inside the sweep window; coalescing (220 since the last sweep)`,
		`... Reconciler: skipped 584 removals for unknown paths in 60.2s [19073 total], sample: /Users/x`,
		`... Reconciler: skipped 122 events escalated for missing parents in 92.2s [39574 total], sample:`,
		`... nothing to see here`,
	}
	e := newEventWork()
	matched := 0
	for _, l := range lines {
		if e.addLine(l) {
			matched++
		}
	}
	if matched != 3 {
		t.Fatalf("matched %d, want 3", matched)
	}
	if e.CoalescedAnchors != 1 || e.SkippedEvents != 706 {
		t.Errorf("got %+v", e)
	}
	if e.SkippedByReason["removals for unknown paths"] != 584 {
		t.Errorf("by reason = %v", e.SkippedByReason)
	}
	// The counters are cumulative over the tailer's life, so a phase diffs them.
	base := newEventWork()
	base.addLine(lines[0])
	d := e.sub(base)
	if d.CoalescedAnchors != 0 || d.SkippedEvents != 706 {
		t.Errorf("sub = %+v", d)
	}
}

func TestWriterRollup(t *testing.T) {
	// Verbatim lines from a real cmdr.log, including the singular form.
	w := newWriterRollup()
	if !w.addLine(`... Writer: +271 msgs (242 upserts, 5 deletes, 7 delete_subtrees, 7 flushes, 10 others) in 5.0s [383707 total]`) {
		t.Fatal("first line did not match")
	}
	if !w.addLine(`... Writer: +379 msgs (345 upserts, 2 moves, 1 delete, 11 flushes) in 5.5s [385327 total]`) {
		t.Fatal("second line did not match")
	}
	// Total is the cumulative counter, so it is the LAST value, not a sum.
	if w.Total != 385327 {
		t.Errorf("Total = %d, want 385327", w.Total)
	}
	// Singular and plural fold to one key: `1 delete` joins `5 deletes`.
	if w.ByKind["upsert"] != 587 || w.ByKind["delete"] != 6 || w.ByKind["move"] != 2 {
		t.Errorf("ByKind = %v", w.ByKind)
	}
	base := newWriterRollup()
	base.addLine(`... Writer: +271 msgs (242 upserts) in 5.0s [383707 total]`)
	d := w.sub(base)
	if d.Total != 1620 || d.ByKind["upsert"] != 345 {
		t.Errorf("sub = %+v", d)
	}
	if w.addLine("not a writer line") {
		t.Error("matched a non-writer line")
	}
}

func TestSingularize(t *testing.T) {
	// Every kind the writer's rollup can print, in both forms.
	cases := map[string]string{
		"upsert": "upsert", "upserts": "upsert",
		"move": "move", "moves": "move",
		"delete": "delete", "deletes": "delete",
		"delete_subtree": "delete_subtree", "delete_subtrees": "delete_subtree",
		"propagation": "propagation", "propagations": "propagation",
		"aggregate": "aggregate", "aggregates": "aggregate",
		"partial aggregate": "partial aggregate", "partial aggregates": "partial aggregate",
		// The one a plain TrimSuffix("s") gets wrong, splitting one counter in two.
		"flush": "flush", "flushes": "flush",
		"other": "other", "others": "other",
	}
	for in, want := range cases {
		if got := singularize(in); got != want {
			t.Errorf("singularize(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestWriterCPU(t *testing.T) {
	// The heartbeat text is pinned on the Rust side by
	// `indexing::writer::tests::the_heartbeat_line_carries_the_fields_the_churn_harness_scrapes`;
	// this is that string with a log prefix in front, as it appears in cmdr.log.
	const prefix = `2026-08-06 09:12:03.114 DEBUG [stall_probe::writer] `
	w := newWriterCPU()
	if !w.addLine(prefix + `heartbeat volume_id=root queue_depth=3 since_last_heartbeat_ms=5004 ` +
		`messages_processed_since_last_heartbeat=271 transaction_commits_since_last_heartbeat=12 ` +
		`time_in_recv_ms=4312 time_in_processing_ms=603 time_in_commit_ms=88 writer_cpu_ms_total=41231`) {
		t.Fatal("the root heartbeat did not match")
	}
	// A second volume's writer has its own thread and its own counter; folding
	// them would make a NAS reconnect look like boot-disk write-path work.
	if !w.addLine(prefix + `heartbeat volume_id=smb-naspolya queue_depth=0 since_last_heartbeat_ms=60003 ` +
		`messages_processed_since_last_heartbeat=0 transaction_commits_since_last_heartbeat=0 ` +
		`time_in_recv_ms=60000 time_in_processing_ms=0 time_in_commit_ms=0 writer_cpu_ms_total=907`) {
		t.Fatal("the SMB heartbeat did not match")
	}
	// Cumulative, so a later line REPLACES the running total rather than adding.
	w.addLine(prefix + `heartbeat volume_id=root queue_depth=0 since_last_heartbeat_ms=5001 ` +
		`messages_processed_since_last_heartbeat=9 transaction_commits_since_last_heartbeat=1 ` +
		`time_in_recv_ms=4900 time_in_processing_ms=80 time_in_commit_ms=20 writer_cpu_ms_total=41880`)
	if w.RootMs() != 41880 || w.MsByVolume["smb-naspolya"] != 907 {
		t.Errorf("MsByVolume = %v", w.MsByVolume)
	}

	base := newWriterCPU()
	base.addLine(prefix + `heartbeat volume_id=root queue_depth=0 since_last_heartbeat_ms=5000 ` +
		`messages_processed_since_last_heartbeat=1 transaction_commits_since_last_heartbeat=1 ` +
		`time_in_recv_ms=5000 time_in_processing_ms=0 time_in_commit_ms=0 writer_cpu_ms_total=40000`)
	d := w.sub(base)
	if d.RootMs() != 1880 {
		t.Errorf("root delta = %d, want 1880", d.RootMs())
	}
	// A volume with no BEFORE reading is dropped, not reported as having burned
	// its whole since-launch total during this phase.
	if _, present := d.MsByVolume["smb-naspolya"]; present {
		t.Errorf("a volume unseen at phase start must not appear: %v", d.MsByVolume)
	}
	if w.addLine(prefix + `Writer: +271 msgs (242 upserts) in 5.0s [383707 total]`) {
		t.Error("matched a writer rollup line")
	}
}
