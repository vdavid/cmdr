// Churn analysis: turns the indexing::churn rollup logs into per-subtree time series, separation times, and ancestor-chain churn shares.
//
// Reads Cmdr log files containing `indexing::churn` rollup lines (emitted when
// the app runs with CMDR_CHURN_SPIKE=1) and answers the spike's three questions:
//
//	Q1  How fast does a hard-churning subtree separate from background noise?
//	Q2  Is there a visible ratio-drop boundary along a real ancestor chain?
//	Q3  What seal-fast / unseal-slow hysteresis windows does the data suggest?
//
// Usage:
//
//	go run ./scripts/churn-analysis [flags] <logfile|glob> [more...]
//
// Full handover, including the exact log format: docs/notes/churn-observability-spike.md
package main

import (
	"bufio"
	"flag"
	"fmt"
	"math"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"
)

// ── Parsed records ───────────────────────────────────────────────────

// periodRec is one `churn_period` summary line.
type periodRec struct {
	seq        uint64
	tMs        int64
	vol        string
	periodMs   uint64
	rawEvents  uint64
	batchPaths uint64
	nodes      uint64
	dropped    uint64
	deepTrunc  uint64
}

// nodeRec is one `churn_node` line: a directory's rolled-up churn in a period.
type nodeRec struct {
	tMs      int64
	vol      string
	path     string
	events   uint64
	direct   uint64
	children uint64
	capped   bool
}

// period is a period line plus the node lines that share its (vol, t_ms) key.
type period struct {
	periodRec
	byPath map[string]nodeRec
}

// key identifies a period across restarts (seq restarts at 0 per live loop, the
// wall-clock stamp does not).
type key struct {
	vol string
	tMs int64
}

func main() {
	var (
		factor    = flag.Float64("factor", 10, "hot threshold: a node is hot when its rolled-up events are this many times the period's average per-directory churn")
		minPeak   = flag.Uint64("min-peak", 20, "ignore nodes whose peak per-period rolled-up events never reach this")
		chains    = flag.Int("chains", 5, "how many hot chains to print for Q2")
		vol       = flag.String("vol", "", "restrict to one volume id (default: every volume found)")
		csvOut    = flag.String("csv", "", "also write the full per-node time series to this CSV path")
		sustained = flag.Int("sustained", 2, "consecutive hot periods required to call a subtree separated (Q1)")
	)
	flag.Parse()
	if flag.NArg() == 0 {
		fmt.Fprintln(os.Stderr, "usage: go run ./scripts/churn-analysis [flags] <logfile|glob> [more...]")
		flag.PrintDefaults()
		os.Exit(2)
	}

	files, err := expand(flag.Args())
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if len(files) == 0 {
		fmt.Fprintln(os.Stderr, "no log files matched")
		os.Exit(1)
	}

	periods, err := parse(files, *vol)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if len(periods) == 0 {
		fmt.Fprintln(os.Stderr, "no `indexing::churn` rollup lines found — was the app run with CMDR_CHURN_SPIKE=1?")
		os.Exit(1)
	}
	sort.Slice(periods, func(i, j int) bool {
		if periods[i].vol != periods[j].vol {
			return periods[i].vol < periods[j].vol
		}
		return periods[i].tMs < periods[j].tMs
	})

	if *csvOut != "" {
		if err := writeCSV(*csvOut, periods); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		fmt.Printf("Wrote per-node time series to %s\n\n", *csvOut)
	}

	for _, v := range volumes(periods) {
		vp := filterVol(periods, v)
		printCollection(v, vp)
		printQ1(vp, *factor, *minPeak, *sustained)
		printQ2(vp, *factor, *minPeak, *chains)
		printQ3(vp, *factor, *minPeak)
	}
}

// ── Parsing ──────────────────────────────────────────────────────────

func expand(args []string) ([]string, error) {
	var out []string
	for _, a := range args {
		matches, err := filepath.Glob(a)
		if err != nil {
			return nil, fmt.Errorf("bad glob %q: %w", a, err)
		}
		if len(matches) == 0 {
			// Not a glob: let the open below report a missing file honestly.
			out = append(out, a)
			continue
		}
		out = append(out, matches...)
	}
	return out, nil
}

// parse reads every file, keeping only churn rollup lines. Node lines are
// matched to their period by (vol, t_ms), which both line kinds carry, so log
// rotation, interleaved volumes, and app restarts all sort themselves out.
func parse(files []string, onlyVol string) ([]period, error) {
	byKey := map[key]*period{}
	for _, f := range files {
		fh, err := os.Open(f)
		if err != nil {
			return nil, fmt.Errorf("open %s: %w", f, err)
		}
		sc := bufio.NewScanner(fh)
		sc.Buffer(make([]byte, 0, 1<<16), 1<<22)
		for sc.Scan() {
			line := sc.Text()
			if i := strings.Index(line, "churn_period "); i >= 0 {
				p := parsePeriod(line[i:])
				if onlyVol != "" && p.vol != onlyVol {
					continue
				}
				k := key{p.vol, p.tMs}
				if existing, ok := byKey[k]; ok {
					existing.periodRec = p
					continue
				}
				byKey[k] = &period{periodRec: p, byPath: map[string]nodeRec{}}
				continue
			}
			if i := strings.Index(line, "churn_node "); i >= 0 {
				n := parseNode(line[i:])
				if onlyVol != "" && n.vol != onlyVol {
					continue
				}
				k := key{n.vol, n.tMs}
				p, ok := byKey[k]
				if !ok {
					p = &period{periodRec: periodRec{vol: n.vol, tMs: n.tMs}, byPath: map[string]nodeRec{}}
					byKey[k] = p
				}
				p.byPath[n.path] = n
			}
		}
		fh.Close()
		if err := sc.Err(); err != nil {
			return nil, fmt.Errorf("read %s: %w", f, err)
		}
	}
	out := make([]period, 0, len(byKey))
	for _, p := range byKey {
		out = append(out, *p)
	}
	return out, nil
}

// fields splits a `k=v k=v … path=<rest>` payload. `path` is always last on a
// node line precisely because paths contain spaces and `=`, so everything after
// `path=` is taken verbatim.
func fields(payload string) map[string]string {
	out := map[string]string{}
	if i := strings.Index(payload, " path="); i >= 0 {
		out["path"] = payload[i+len(" path="):]
		payload = payload[:i]
	}
	for _, tok := range strings.Fields(payload) {
		k, v, ok := strings.Cut(tok, "=")
		if ok {
			out[k] = v
		}
	}
	return out
}

func parsePeriod(payload string) periodRec {
	f := fields(payload)
	return periodRec{
		seq:        u64(f["seq"]),
		tMs:        int64(u64(f["t_ms"])),
		vol:        f["vol"],
		periodMs:   u64(f["period_ms"]),
		rawEvents:  u64(f["raw_events"]),
		batchPaths: u64(f["batch_paths"]),
		nodes:      u64(f["nodes"]),
		dropped:    u64(f["nodes_dropped"]),
		deepTrunc:  u64(f["deep_truncated"]),
	}
}

func parseNode(payload string) nodeRec {
	f := fields(payload)
	return nodeRec{
		tMs:      int64(u64(f["t_ms"])),
		vol:      f["vol"],
		path:     f["path"],
		events:   u64(f["events"]),
		direct:   u64(f["direct"]),
		children: u64(f["children"]),
		capped:   f["capped"] == "1",
	}
}

func u64(s string) uint64 {
	v, _ := strconv.ParseUint(strings.TrimSpace(s), 10, 64)
	return v
}

// ── Shared helpers ───────────────────────────────────────────────────

func volumes(ps []period) []string {
	seen := map[string]bool{}
	var out []string
	for _, p := range ps {
		if !seen[p.vol] {
			seen[p.vol] = true
			out = append(out, p.vol)
		}
	}
	sort.Strings(out)
	return out
}

func filterVol(ps []period, vol string) []period {
	var out []period
	for _, p := range ps {
		if p.vol == vol {
			out = append(out, p)
		}
	}
	return out
}

// noiseFloor is the period's average churn per tracked directory: total
// deduplicated paths divided by how many directories saw any churn. It is the
// "what does an ordinary background directory look like right now" baseline
// every hot/quiet call in this tool is measured against.
func (p period) noiseFloor() float64 {
	if p.nodes == 0 {
		return 1
	}
	n := float64(p.batchPaths) / float64(p.nodes)
	if n < 1 {
		return 1
	}
	return n
}

// stats holds a node's behaviour across the whole collection window.
type stats struct {
	path       string
	peak       uint64
	total      uint64
	seen       int
	hotPeriods int
	firstSeen  int64
	firstHot   int64
	separated  int64 // t_ms of the first period completing a sustained hot run
	maxChild   uint64
	everCapped bool
	hotRuns    []int
	quietGaps  []int
}

// summarize walks the periods in time order and derives per-node stats.
func summarize(ps []period, factor float64, sustainedNeeded int) map[string]*stats {
	out := map[string]*stats{}
	run := map[string]int{}
	gap := map[string]int{}
	for _, p := range ps {
		hotNow := observePeriod(out, p, factor)
		trackRuns(out, run, gap, hotNow, p.tMs, sustainedNeeded)
	}
	// Flush open runs so a subtree that is still hot at collection end counts.
	for path, s := range out {
		if run[path] > 0 {
			s.hotRuns = append(s.hotRuns, run[path])
		}
		if gap[path] > 0 {
			s.quietGaps = append(s.quietGaps, gap[path])
		}
	}
	return out
}

// observePeriod folds one period's node lines into `out`, returning which nodes
// were hot in it.
func observePeriod(out map[string]*stats, p period, factor float64) map[string]bool {
	floor := p.noiseFloor()
	hotNow := map[string]bool{}
	for path, n := range p.byPath {
		s, ok := out[path]
		if !ok {
			s = &stats{path: path, firstSeen: p.tMs}
			out[path] = s
		}
		s.seen++
		s.total += n.events
		if n.events > s.peak {
			s.peak = n.events
		}
		if n.children > s.maxChild {
			s.maxChild = n.children
		}
		if n.capped {
			s.everCapped = true
		}
		if float64(n.events) >= factor*floor {
			hotNow[path] = true
			s.hotPeriods++
			if s.firstHot == 0 {
				s.firstHot = p.tMs
			}
		}
	}
	return hotNow
}

// trackRuns extends or closes each node's hot run and quiet gap for one period,
// and stamps the moment a node completes its first sustained hot run.
func trackRuns(out map[string]*stats, run, gap map[string]int, hotNow map[string]bool, tMs int64, sustainedNeeded int) {
	for path, s := range out {
		if hotNow[path] {
			if gap[path] > 0 {
				s.quietGaps = append(s.quietGaps, gap[path])
				gap[path] = 0
			}
			run[path]++
			if run[path] == sustainedNeeded && s.separated == 0 {
				s.separated = tMs
			}
			continue
		}
		if run[path] > 0 {
			s.hotRuns = append(s.hotRuns, run[path])
			run[path] = 0
		}
		if s.firstHot != 0 {
			gap[path]++
		}
	}
}

func ranked(all map[string]*stats, minPeak uint64) []*stats {
	var out []*stats
	for _, s := range all {
		if s.peak >= minPeak {
			out = append(out, s)
		}
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].total != out[j].total {
			return out[i].total > out[j].total
		}
		return out[i].path < out[j].path
	})
	return out
}

func avgPeriodSec(ps []period) float64 {
	var sum, n float64
	for _, p := range ps {
		if p.periodMs > 0 {
			sum += float64(p.periodMs) / 1000
			n++
		}
	}
	if n == 0 {
		return 30
	}
	return sum / n
}

// periodSpread reports the measured period lengths in seconds (min, median,
// max) plus their total. A single mean hides the thing a reader most needs:
// collections are often stitched together from several live-loop runs with
// different `CMDR_CHURN_SPIKE_PERIOD_S` settings, and every timing this tool
// prints is quantised to one period.
func periodSpread(ps []period) (minS, medS, maxS, totalS float64) {
	var v []float64
	for _, p := range ps {
		if p.periodMs > 0 {
			v = append(v, float64(p.periodMs)/1000)
			totalS += float64(p.periodMs) / 1000
		}
	}
	if len(v) == 0 {
		return 0, 0, 0, 0
	}
	sort.Float64s(v)
	return v[0], v[len(v)/2], v[len(v)-1], totalS
}

func ts(ms int64) string {
	if ms == 0 {
		return "—"
	}
	return time.UnixMilli(ms).Format("2006-01-02 15:04:05")
}

func pct(v []int, q float64) int {
	if len(v) == 0 {
		return 0
	}
	s := append([]int(nil), v...)
	sort.Ints(s)
	i := int(q * float64(len(s)-1))
	return s[i]
}

// ── Report sections ──────────────────────────────────────────────────

func printCollection(vol string, ps []period) {
	if len(ps) == 0 {
		return
	}
	var raw, paths, dropped, deep uint64
	maxNodes := uint64(0)
	for _, p := range ps {
		raw += p.rawEvents
		paths += p.batchPaths
		dropped += p.dropped
		deep += p.deepTrunc
		if p.nodes > maxNodes {
			maxNodes = p.nodes
		}
	}
	span := time.Duration(0)
	if len(ps) > 1 {
		span = time.UnixMilli(ps[len(ps)-1].tMs).Sub(time.UnixMilli(ps[0].tMs))
	}
	dedup := 0.0
	if paths > 0 {
		dedup = float64(raw) / float64(paths)
	}
	fmt.Printf("════════ volume %s ════════\n", vol)
	lo, med, hi, total := periodSpread(ps)
	fmt.Printf("Collection: %d periods, %s → %s (wall span %s)\n",
		len(ps), ts(ps[0].tMs), ts(ps[len(ps)-1].tMs), span.Round(time.Minute))
	fmt.Printf("Live coverage: %s of measured periods (%.0f%% of the wall span; the rest is time no live loop ran)\n",
		(time.Duration(total) * time.Second).Round(time.Second), 100*total/math.Max(span.Seconds(), 1))
	fmt.Printf("Period length: %.0f–%.0f s, median %.0f s. Every timing below is quantised to one period.\n",
		lo, hi, med)
	fmt.Printf("Events: %d raw, %d deduplicated paths (%.1f× dedup), peak %d directories tracked in a period\n",
		raw, paths, dedup, maxNodes)
	if dropped > 0 || deep > 0 {
		fmt.Printf("⚠️  Instrumentation caps engaged: %d node insertions dropped, %d paths depth-truncated\n", dropped, deep)
	}
	fmt.Println()
}

func printQ1(ps []period, factor float64, minPeak uint64, sustainedNeeded int) {
	all := summarize(ps, factor, sustainedNeeded)
	top := ranked(all, minPeak)
	lo, med, hi, _ := periodSpread(ps)

	fmt.Println("── Q1. Separation from background noise ──")
	fmt.Printf("A node is \"hot\" when its rolled-up events reach %.0f× the period's average per-directory churn.\n", factor)
	fmt.Printf("\"Separated after\" is the time from a node's first appearance to %d consecutive hot periods.\n\n", sustainedNeeded)
	fmt.Printf("%-12s %10s %10s %8s %8s %14s  %s\n", "TOTAL", "PEAK/prd", "MED/prd", "PERIODS", "HOT", "SEPARATED", "PATH")
	for _, s := range top {
		sep := "never"
		if s.separated != 0 {
			sep = (time.Duration(float64(s.separated-s.firstSeen)) * time.Millisecond).Round(time.Second).String()
		}
		fmt.Printf("%-12d %10d %10s %8d %8d %14s  %s\n",
			s.total, s.peak, medianPerPeriod(ps, s.path), s.seen, s.hotPeriods, sep, s.path)
	}
	if len(top) == 0 {
		fmt.Printf("(nothing reached the --min-peak=%d floor)\n", minPeak)
	}
	fmt.Printf("\nResolution: one period (%.0f–%.0f s here, median %.0f s). A \"SEPARATED\" reading equal to one period\n"+
		"means \"already hot by the next period\", NOT \"in exactly that many seconds\"; the true value is anywhere\n"+
		"from instant to one period. Readings that span a gap between live-loop runs are upper bounds only.\n\n",
		lo, hi, med)
}

func medianPerPeriod(ps []period, path string) string {
	var v []int
	for _, p := range ps {
		if n, ok := p.byPath[path]; ok {
			v = append(v, int(n.events))
		}
	}
	return strconv.Itoa(pct(v, 0.5))
}

// printQ2 walks each hot node's ancestor chain and prints the child/parent
// rolled-up ratio at every step. A "ratio drop" is a step where the child
// carries a small fraction of the parent's churn: the parent holds churn that
// does not come from that child. That boundary is what a seal-root rule needs
// to find: the highest directory whose churn really is all one subtree's.
func printQ2(ps []period, factor float64, minPeak uint64, want int) {
	all := summarize(ps, factor, 1)
	top := ranked(all, minPeak)

	fmt.Println("── Q2. Ratio drop along real ancestor chains ──")
	fmt.Println("For each hot leaf, the chain from `/` down. `share` is this node's churn as a fraction of its parent's;")
	fmt.Println("a low share means the parent holds churn from elsewhere — the seal-root boundary. `children` is the")
	fmt.Printf("distinct direct children that churned (peak in any period; a `+` means it saturated the instrument's cap).\n\n")

	printed := 0
	for _, s := range top {
		if printed >= want {
			break
		}
		// One chain per hot LEAF. A node with a hot descendant is already a row
		// inside that descendant's chain, so printing it again says nothing new.
		if hasHotDescendant(s.path, top) {
			continue
		}
		chain := ancestors(s.path)
		fmt.Printf("Chain to %s\n", s.path)
		fmt.Printf("  %12s %8s %9s %9s  %s\n", "TOTAL", "SHARE", "DIRECT", "CHILDREN", "PATH")
		var prev uint64
		var maxDropAt string
		var maxDropShare = 2.0
		for _, a := range chain {
			st, ok := all[a]
			if !ok {
				fmt.Printf("  %12s %8s %9s %9s  %s\n", "—", "—", "—", "—", a+"  (never ranked into the emitted top-N)")
				continue
			}
			share := ""
			if prev > 0 {
				sh := float64(st.total) / float64(prev)
				share = fmt.Sprintf("%.3f", sh)
				if sh < maxDropShare {
					maxDropShare = sh
					maxDropAt = a
				}
			}
			capMark := ""
			if st.everCapped {
				capMark = "+"
			}
			fmt.Printf("  %12d %8s %9d %8d%-1s  %s\n", st.total, share, directTotal(ps, a), st.maxChild, capMark, a)
			prev = st.total
		}
		if maxDropAt != "" && maxDropShare < 1 {
			fmt.Printf("  ➜ steepest drop entering %s (share %.3f): everything below it is churn the parent does not share.\n", maxDropAt, maxDropShare)
		} else {
			fmt.Printf("  ➜ no drop: churn is uniform all the way to the root on this chain.\n")
		}
		fmt.Println()
		printed++
	}
	if printed == 0 {
		fmt.Println("(no chain reached the --min-peak floor)")
		fmt.Println()
	}
}

func directTotal(ps []period, path string) uint64 {
	var t uint64
	for _, p := range ps {
		if n, ok := p.byPath[path]; ok {
			t += n.direct
		}
	}
	return t
}

// hasHotDescendant reports whether any other ranked node lives strictly under
// `p`. Component-aware, so `/a/bc` never counts as a child of `/a/b`.
func hasHotDescendant(p string, top []*stats) bool {
	prefix := strings.TrimSuffix(p, "/") + "/"
	for _, s := range top {
		if s.path != p && strings.HasPrefix(s.path, prefix) {
			return true
		}
	}
	return false
}

// ancestors returns `/`, then every ancestor of p, then p itself.
func ancestors(p string) []string {
	out := []string{"/"}
	cur := "/"
	for _, c := range strings.Split(strings.Trim(p, "/"), "/") {
		if c == "" {
			continue
		}
		cur = path.Join(cur, c)
		out = append(out, cur)
	}
	return out
}

// printQ3 turns hot-run and quiet-gap lengths into candidate hysteresis
// constants: seal-fast wants to fire inside a typical hot run, unseal-slow must
// outlast a typical quiet gap or the same subtree thrashes.
func printQ3(ps []period, factor float64, minPeak uint64) {
	all := summarize(ps, factor, 1)
	top := ranked(all, minPeak)
	secs := avgPeriodSec(ps)

	fmt.Println("── Q3. Hysteresis constants suggested by the data ──")
	fmt.Printf("Durations here are period COUNTS × the mean period (%.0f s), so they are coarser than Q1's\n"+
		"timestamp-derived figures. Read them as \"about N periods\", not as wall-clock seconds.\n", secs)
	fmt.Printf("Hot run = consecutive hot periods. Quiet gap = consecutive non-hot periods after a node's first hot period.\n\n")
	fmt.Printf("%9s %9s %9s %9s %9s %9s  %s\n", "RUNS", "RUN p50", "RUN p90", "GAPS", "GAP p50", "GAP p90", "PATH")
	var allRuns, allGaps []int
	for _, s := range top {
		allRuns = append(allRuns, s.hotRuns...)
		allGaps = append(allGaps, s.quietGaps...)
		fmt.Printf("%9d %9s %9s %9d %9s %9s  %s\n",
			len(s.hotRuns), dur(pct(s.hotRuns, 0.5), secs), dur(pct(s.hotRuns, 0.9), secs),
			len(s.quietGaps), dur(pct(s.quietGaps, 0.5), secs), dur(pct(s.quietGaps, 0.9), secs), s.path)
	}
	fmt.Println()
	if len(allRuns) > 0 {
		fmt.Printf("Across every hot node: hot run p50 %s / p90 %s; quiet gap p50 %s / p90 %s / max %s.\n",
			dur(pct(allRuns, 0.5), secs), dur(pct(allRuns, 0.9), secs),
			dur(pct(allGaps, 0.5), secs), dur(pct(allGaps, 0.9), secs), dur(maxInt(allGaps), secs))
		fmt.Printf("Read: seal-fast should fire well inside %s (the p50 hot run); unseal-slow must exceed %s\n",
			dur(pct(allRuns, 0.5), secs), dur(pct(allGaps, 0.9), secs))
		fmt.Println("(the p90 quiet gap) or a genuinely churny subtree unseals and re-seals on its own idle pauses.")
	} else {
		fmt.Println("(no hot runs observed — either the window was too quiet or --factor is too high)")
	}
	fmt.Println()
}

func dur(periods int, secs float64) string {
	if periods == 0 {
		return "—"
	}
	return (time.Duration(float64(periods)*secs) * time.Second).Round(time.Second).String()
}

func maxInt(v []int) int {
	m := 0
	for _, x := range v {
		if x > m {
			m = x
		}
	}
	return m
}

// ── CSV ──────────────────────────────────────────────────────────────

// writeCSV dumps every node observation, one row per (period, node), for
// plotting elsewhere. `path` is last so a comma in a path can't shift columns.
func writeCSV(dest string, ps []period) error {
	f, err := os.Create(dest)
	if err != nil {
		return fmt.Errorf("create %s: %w", dest, err)
	}
	defer f.Close()
	w := bufio.NewWriter(f)
	defer w.Flush()
	fmt.Fprintln(w, "t_ms,iso,vol,seq,period_ms,raw_events,batch_paths,nodes,events,direct,children,capped,path")
	for _, p := range ps {
		paths := make([]string, 0, len(p.byPath))
		for path := range p.byPath {
			paths = append(paths, path)
		}
		sort.Strings(paths)
		for _, path := range paths {
			n := p.byPath[path]
			capped := 0
			if n.capped {
				capped = 1
			}
			fmt.Fprintf(w, "%d,%s,%s,%d,%d,%d,%d,%d,%d,%d,%d,%d,%s\n",
				p.tMs, time.UnixMilli(p.tMs).Format(time.RFC3339), p.vol, p.seq, p.periodMs,
				p.rawEvents, p.batchPaths, p.nodes, n.events, n.direct, n.children, capped, path)
		}
	}
	return nil
}
