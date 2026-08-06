package main

import (
	"bufio"
	"io"
	"maps"
	"os"
	"strings"
	"sync"
	"time"
)

// Tailer follows the live `cmdr.log` from wherever it stood when the tailer
// started, counting bytes and folding rebuild summaries into per-phase counters.
//
// It handles the 50 MB rotation, which a naive "size at end minus size at start"
// would report as a NEGATIVE log volume: when the file shrinks or its identity
// changes, the remainder of the old file is drained first, then the new one is
// followed from zero. Log volume matters here because these subtrees are also
// where the log lines come from.
type Tailer struct {
	path string
	// repoPrefix classifies each rebuild as ours or somebody else's. On a machine
	// running several build trees at once (agents in sibling worktrees, say), the
	// app reconciles all of them, and counting the lot as "what our churn cost"
	// would inflate the baseline by an amount that varies run to run.
	repoPrefix string
	// markedRoots are the `CACHEDIR.TAG` subtree roots, so a rebuild can be counted
	// as inside or outside the set the feature will change. That ratio is the
	// plan's central claim, and it is the one number an after-run must move.
	markedRoots []string

	// partial holds a line the app hadn't finished writing when we caught up; it
	// is prepended to the next read so a split write is never parsed as two lines.
	partial string

	mu         sync.Mutex
	bytesRead  int64
	lines      int64
	rotations  int
	writes     RowWrites // every rebuild the app reported
	repoWrite  RowWrites // only rebuilds under repoPrefix
	markedWrit RowWrites // only rebuilds inside a marked subtree
	aggregate  AggregateWrites
	eventWork  EventWork
	writerRoll WriterRollup
	writerCPU  WriterCPU
}

// Snapshot is a Tailer's counters at one instant, so a phase can diff them.
type Snapshot struct {
	Bytes      int64
	Lines      int64
	Rotations  int
	Writes     RowWrites
	RepoWrite  RowWrites
	MarkedWrit RowWrites
	Aggregate  AggregateWrites
	EventWork  EventWork
	WriterRoll WriterRollup
	WriterCPU  WriterCPU
}

// snapshot copies the counters, DEEP-copying the two maps. A shallow copy shares
// the map with the still-running tailer, so a phase's "end minus start" would
// subtract the end values from themselves and every per-kind delta would come out
// zero.
func (t *Tailer) snapshot() Snapshot {
	t.mu.Lock()
	defer t.mu.Unlock()
	eventWork := t.eventWork
	eventWork.SkippedByReason = maps.Clone(t.eventWork.SkippedByReason)
	writerRoll := t.writerRoll
	writerRoll.ByKind = maps.Clone(t.writerRoll.ByKind)
	writerCPU := t.writerCPU
	writerCPU.MsByVolume = maps.Clone(t.writerCPU.MsByVolume)
	return Snapshot{
		Bytes:      t.bytesRead,
		Lines:      t.lines,
		Rotations:  t.rotations,
		Writes:     t.writes,
		RepoWrite:  t.repoWrite,
		MarkedWrit: t.markedWrit,
		Aggregate:  t.aggregate,
		EventWork:  eventWork,
		WriterRoll: writerRoll,
		WriterCPU:  writerCPU,
	}
}

// sub returns the counters accumulated between two snapshots.
func (s Snapshot) sub(o Snapshot) Snapshot {
	return Snapshot{
		Bytes:      s.Bytes - o.Bytes,
		Lines:      s.Lines - o.Lines,
		Rotations:  s.Rotations - o.Rotations,
		Writes:     s.Writes.sub(o.Writes),
		RepoWrite:  s.RepoWrite.sub(o.RepoWrite),
		MarkedWrit: s.MarkedWrit.sub(o.MarkedWrit),
		Aggregate:  s.Aggregate.sub(o.Aggregate),
		EventWork:  s.EventWork.sub(o.EventWork),
		WriterRoll: s.WriterRoll.sub(o.WriterRoll),
		WriterCPU:  s.WriterCPU.sub(o.WriterCPU),
	}
}

// follow reads from the end of the file until done closes. Lines already in the
// file when it starts are NOT counted: a phase measures what the app wrote during
// the phase.
func (t *Tailer) follow(done <-chan struct{}) {
	f, err := os.Open(t.path)
	if err != nil {
		return
	}
	defer f.Close()
	offset, err := f.Seek(0, io.SeekEnd)
	if err != nil {
		return
	}
	reader := bufio.NewReaderSize(f, 1<<16)

	for {
		select {
		case <-done:
			t.drain(reader)
			return
		default:
		}
		n := t.drain(reader)
		offset += n
		if n == 0 {
			// Caught up. A file that is now SHORTER than our offset has rotated
			// under us; reopen and follow the new one from its start.
			if info, statErr := os.Stat(t.path); statErr == nil && info.Size() < offset {
				nf, openErr := os.Open(t.path)
				if openErr == nil {
					f.Close()
					f = nf
					reader = bufio.NewReaderSize(f, 1<<16)
					offset = 0
					t.partial = ""
					t.mu.Lock()
					t.rotations++
					t.mu.Unlock()
					continue
				}
			}
			time.Sleep(200 * time.Millisecond)
		}
	}
}

// drain consumes every complete line currently available, returning the bytes it
// consumed. A trailing partial line is carried over rather than dropped, so a
// write caught mid-flight isn't counted as two lines on the next pass.
func (t *Tailer) drain(reader *bufio.Reader) int64 {
	var consumed int64
	for {
		chunk, err := reader.ReadString('\n')
		consumed += int64(len(chunk))
		if err != nil {
			t.partial += chunk
			return consumed
		}
		line := t.partial + chunk
		t.partial = ""
		t.mu.Lock()
		t.bytesRead += int64(len(line))
		t.lines++
		if b, ok := parseRebuildLine(line); ok {
			t.writes.add(b)
			if t.repoPrefix != "" && strings.HasPrefix(b.Path, t.repoPrefix) {
				t.repoWrite.add(b)
			}
			if underAny(b.Path, t.markedRoots) {
				t.markedWrit.add(b)
			}
		} else if a, ok := parseAggregateLine(line); ok {
			t.aggregate.add(a)
		} else if !t.writerRoll.addLine(line) && !t.writerCPU.addLine(line) {
			t.eventWork.addLine(line)
		}
		t.mu.Unlock()
	}
}

// underAny reports whether path is one of the roots or sits inside one. The
// separator check keeps `/a/target` from matching a root `/a/tar`.
func underAny(path string, roots []string) bool {
	for _, r := range roots {
		if path == r || strings.HasPrefix(path, strings.TrimSuffix(r, "/")+"/") {
			return true
		}
	}
	return false
}
