package checks

import (
	"path/filepath"
	"reflect"
	"sync/atomic"
	"testing"
	"time"
)

// Findings are joined in module order, so results must come back in input order even when
// later items finish first.
func TestRunPerModuleKeepsInputOrder(t *testing.T) {
	items := []int{5, 4, 3, 2, 1, 0}
	got := runPerModule(items, 3, func(n int) int {
		time.Sleep(time.Duration(n) * 5 * time.Millisecond)
		return n * 10
	})
	want := []int{50, 40, 30, 20, 10, 0}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

// The cap is what keeps a local `pnpm check` from holding several multi-GB nilaway
// processes at once.
func TestRunPerModuleRespectsWorkerCap(t *testing.T) {
	var running, peak atomic.Int32
	items := make([]int, 12)
	runPerModule(items, 2, func(int) struct{} {
		n := running.Add(1)
		for {
			p := peak.Load()
			if n <= p || peak.CompareAndSwap(p, n) {
				break
			}
		}
		time.Sleep(10 * time.Millisecond)
		running.Add(-1)
		return struct{}{}
	})
	if got := peak.Load(); got != 2 {
		t.Fatalf("peak concurrency %d, want 2", got)
	}
}

func TestRunPerModuleHandlesEmptyAndBadWorkerCount(t *testing.T) {
	if got := runPerModule([]int{}, 2, func(n int) int { return n }); len(got) != 0 {
		t.Fatalf("empty input gave %v", got)
	}
	if got := runPerModule([]int{1, 2}, 0, func(n int) int { return n + 1 }); !reflect.DeepEqual(got, []int{2, 3}) {
		t.Fatalf("zero workers gave %v", got)
	}
}

// FindAllGoModules returns a map, whose iteration order is random; the module list must
// follow GetGoDirectories so the output doesn't reshuffle between runs.
func TestNilawayModulesFollowGoDirectoryOrder(t *testing.T) {
	dirs := GetGoDirectories()
	all := map[string][]string{}
	for _, d := range dirs {
		all[d] = []string{"b", "a"}
	}
	got := nilawayModules("/root", all)
	var want []nilawayModule
	for _, d := range dirs {
		for _, m := range []string{"b", "a"} {
			want = append(want, nilawayModule{dir: filepath.Join("/root", d, m), label: filepath.Join(d, m)})
		}
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}
