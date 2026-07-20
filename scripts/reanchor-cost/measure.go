package main

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"syscall"
	"time"
)

// Method is one way of walking a directory one level deep.
type Method string

const (
	// MethodEnumerate reads names only: the floor any re-anchor pays.
	MethodEnumerate Method = "enumerate"
	// MethodLstat reads names plus one lstat per entry: the naive re-anchor.
	MethodLstat Method = "lstat"
	// MethodBulk reads names and sizes in batches via getattrlistbulk (macOS).
	MethodBulk Method = "bulk"
)

// Result is one timed pass over one directory.
type Result struct {
	Dir           string
	Method        Method
	Run           int
	Duration      time.Duration
	CPUTime       time.Duration // user + system time this pass burned
	Entries       int64         // children, excluding "." and ".."
	Dirs          int64         // of those, directories
	LogicalBytes  int64         // summed over non-directory children (0 for enumerate)
	PhysicalBytes int64         // allocated bytes, same population
}

// MicrosPerEntry is the derived per-entry cost the spike compares across
// directories.
func (r Result) MicrosPerEntry() float64 {
	if r.Entries == 0 {
		return 0
	}
	return float64(r.Duration.Microseconds()) / float64(r.Entries)
}

// CPUPercent is CPU time over wall time. Near 100% means the pass is compute or
// syscall bound; far below means it spent the difference waiting on IO.
func (r Result) CPUPercent() float64 {
	if r.Duration == 0 {
		return 0
	}
	return 100 * r.CPUTime.Seconds() / r.Duration.Seconds()
}

// Measure times one method over one directory. Timing starts before the open so
// every method pays the same setup, matching what a re-anchor would actually
// cost end to end.
func Measure(m Method, dir string, bulkBufBytes int) (Result, error) {
	cpuStart := cpuTime()
	start := time.Now()
	var res Result
	var err error
	switch m {
	case MethodEnumerate:
		res, err = measureEnumerate(dir)
	case MethodLstat:
		res, err = measureLstat(dir)
	case MethodBulk:
		res, err = measureBulk(dir, bulkBufBytes)
	default:
		return Result{}, fmt.Errorf("unknown method %q", m)
	}
	if err != nil {
		return Result{}, err
	}
	res.Method = m
	res.Duration = time.Since(start)
	res.CPUTime = cpuTime() - cpuStart
	return res, nil
}

// cpuTime is the process's user + system time so far. The wall-to-CPU ratio is
// the diagnostic that separates "this walk is syscall-bound" from "this walk is
// waiting on metadata reads from disk", which is the difference between a cost
// a faster syscall can fix and one it cannot.
func cpuTime() time.Duration {
	var ru syscall.Rusage
	if err := syscall.Getrusage(syscall.RUSAGE_SELF, &ru); err != nil {
		return 0
	}
	toDuration := func(tv syscall.Timeval) time.Duration {
		return time.Duration(tv.Sec)*time.Second + time.Duration(tv.Usec)*time.Microsecond
	}
	return toDuration(ru.Utime) + toDuration(ru.Stime)
}

// readdirBatch is how many names we pull per readdir call. Large enough that the
// syscall count is not what we're measuring, small enough that the whole
// directory never has to be materialized at once (1.4M `os.DirEntry`s would be
// hundreds of MB, and a re-anchor must stream).
const readdirBatch = 4096

func measureEnumerate(dir string) (Result, error) {
	f, err := os.Open(dir)
	if err != nil {
		return Result{}, err
	}
	defer func() { _ = f.Close() }()

	var res Result
	for {
		batch, err := f.ReadDir(readdirBatch)
		for _, e := range batch {
			res.Entries++
			if e.IsDir() {
				res.Dirs++
			}
		}
		if err == io.EOF {
			break
		}
		if err != nil {
			return Result{}, err
		}
		if len(batch) == 0 {
			break
		}
	}
	return res, nil
}

func measureLstat(dir string) (Result, error) {
	f, err := os.Open(dir)
	if err != nil {
		return Result{}, err
	}
	defer func() { _ = f.Close() }()

	var res Result
	var st syscall.Stat_t
	for {
		batch, err := f.ReadDir(readdirBatch)
		for _, e := range batch {
			res.Entries++
			path := filepath.Join(dir, e.Name())
			if lerr := syscall.Lstat(path, &st); lerr != nil {
				// A churny directory loses entries between readdir and lstat.
				// That is normal, not a measurement failure: skip the entry.
				continue
			}
			if st.Mode&syscall.S_IFMT == syscall.S_IFDIR {
				res.Dirs++
				continue
			}
			res.LogicalBytes += st.Size
			res.PhysicalBytes += st.Blocks * 512
		}
		if err == io.EOF {
			break
		}
		if err != nil {
			return Result{}, err
		}
		if len(batch) == 0 {
			break
		}
	}
	return res, nil
}
