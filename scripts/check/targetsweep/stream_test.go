package targetsweep

import (
	"sort"
	"strings"
	"testing"
	"time"
)

// Every entry cargo writes under `deps/`, `.fingerprint/`, `build/`, and `examples/` ends
// in its unit's 16-hex hash, optionally followed by an extension. Names copied from a
// real cargo 1.99.1 volume (2026-11-12); `TestCargoLayoutMatchesTheSweepsAssumptions`
// pins the same shape against a live cargo.
func TestUnitHashReadsCargosEntryNames(t *testing.T) {
	cases := []struct {
		name, want string
	}{
		{"libserde-0123456789abcdef.rlib", "0123456789abcdef"},
		{"libserde-0123456789abcdef.rmeta", "0123456789abcdef"},
		{"serde-0123456789abcdef.d", "0123456789abcdef"},
		{"cmdr_lib-0123456789abcdef", "0123456789abcdef"},
		{"libtauri_plugin_mcp_bridge-0123456789abcdef.a", "0123456789abcdef"},
		{"libserde_derive-0123456789abcdef.so", "0123456789abcdef"},
		{"tauri-plugin-mcp-bridge-0123456789abcdef", "0123456789abcdef"},
		{"copy_xattrs_bench-03b1c63b95c7deeb", "03b1c63b95c7deeb"},
		{"cmdr_lib-0123456789abcdef.cmdr_lib.a1b2-cgu.0.rcgu.o", "0123456789abcdef"},
		// A name that itself ends like a hash still yields the last one.
		{"foo-aaaaaaaaaaaaaaaa-0123456789abcdef.rlib", "0123456789abcdef"},
		// Cargo's unhashed outputs (cdylib, staticlib, uplifted copies) and anything else.
		{"libcmdr_lib.so", ""},
		{"libcmdr_lib.a", ""},
		{"copy_xattrs_bench", ""},
		{"copy_xattrs_bench.d", ""},
		{".cargo-lock", ""},
		{"serde-0123456789ABCDEF", ""},
		{"serde-0123456789abcde", ""},
		{"serde-0123456789abcdef0", ""},
		{"serde_0123456789abcdef", ""},
		{"-0123456789abcdef", ""},
	}
	for _, c := range cases {
		got, ok := unitHash(c.name)
		if got != c.want || ok != (c.want != "") {
			t.Errorf("unitHash(%q) = %q, %v; want %q", c.name, got, ok, c.want)
		}
	}
}

const finished = `{"reason":"build-finished","success":true}`

func artifactLine(paths ...string) string {
	return `{"reason":"compiler-artifact","package_id":"registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0","filenames":["` +
		strings.Join(paths, `","`) + `"],"executable":null,"fresh":true}`
}

func TestParseStreamCollectsEveryUnitHash(t *testing.T) {
	stream := strings.Join([]string{
		artifactLine("/cargo-target/debug/deps/libserde-1111111111111111.rlib", "/cargo-target/debug/deps/libserde-1111111111111111.rmeta"),
		`{"reason":"compiler-artifact","filenames":["/cargo-target/debug/deps/cmdr_lib-2222222222222222"],"executable":"/cargo-target/debug/deps/cmdr_lib-2222222222222222","fresh":false}`,
		`{"reason":"compiler-artifact","filenames":["/cargo-target/debug/build/ring-3333333333333333/build-script-build"],"executable":null,"fresh":true}`,
		`{"reason":"build-script-executed","package_id":"x","out_dir":"/cargo-target/debug/build/ring-4444444444444444/out"}`,
		// An uplifted bin names no hash; the hard-link rule covers it.
		`{"reason":"compiler-artifact","filenames":["/cargo-target/debug/cmdr"],"executable":"/cargo-target/debug/cmdr","fresh":true}`,
		`{"reason":"compiler-message","message":{"rendered":"warning: x"}}`,
		"    Checking serde v1.0.0", // a text line in a combined stream (the doc lane's)
		finished,
	}, "\n") + "\n"

	got := parseStream([]byte(stream))
	if !got.complete || got.malformed != 0 || got.artifacts != 4 {
		t.Fatalf("complete=%v malformed=%d artifacts=%d, want true/0/4", got.complete, got.malformed, got.artifacts)
	}
	want := []string{"1111111111111111", "2222222222222222", "3333333333333333", "4444444444444444"}
	if keys := sortedKeys(got.hashes); strings.Join(keys, ",") != strings.Join(want, ",") {
		t.Errorf("hashes = %v, want %v", keys, want)
	}
}

func TestParseStreamOnlyAFinishedSuccessfulBuildIsComplete(t *testing.T) {
	art := artifactLine("/t/debug/deps/liba-1111111111111111.rlib")
	cases := []struct {
		name, stream string
		complete     bool
		malformed    int
	}{
		{"finished", art + "\n" + finished + "\n", true, 0},
		{"failed build", art + "\n" + `{"reason":"build-finished","success":false}` + "\n", false, 0},
		{"killed mid-run", art + "\n", false, 0},
		{"cut mid-line", art + "\n" + `{"reason":"compiler-art`, false, 1},
		{"a fake finish in text", art + "\n" + `error: "reason":"build-finished","success":true` + "\n", false, 0},
		{"empty", "", false, 0},
	}
	for _, c := range cases {
		got := parseStream([]byte(c.stream))
		if got.complete != c.complete || got.malformed != c.malformed {
			t.Errorf("%s: complete=%v malformed=%d, want %v/%d", c.name, got.complete, got.malformed, c.complete, c.malformed)
		}
	}
}

var t0 = time.Date(2026, 11, 12, 12, 0, 0, 0, time.UTC)

func stream(name string, at time.Time, lines ...string) StreamFile {
	return StreamFile{Name: name, ModTime: at, Content: []byte(strings.Join(lines, "\n") + "\n")}
}

func allComplete(at time.Time) []StreamFile {
	return []StreamFile{
		stream("clippy.jsonl", at, artifactLine("/t/debug/deps/liba-1111111111111111.rmeta"), finished),
		stream("doc.jsonl", at, artifactLine("/t/debug/deps/libb-2222222222222222.rmeta"), finished),
		stream("tests.jsonl", at, artifactLine("/t/debug/deps/libc-3333333333333333.rlib"), finished),
	}
}

func TestAssembleLiveUnionsEveryLanesCompleteStream(t *testing.T) {
	live, superseded := assembleLive(allComplete(t0), Lanes)
	if live.Problem != "" {
		t.Fatalf("unexpected problem: %s", live.Problem)
	}
	if got := strings.Join(sortedKeys(live.Hashes), ","); got != "1111111111111111,2222222222222222,3333333333333333" {
		t.Errorf("hashes = %s", got)
	}
	if len(superseded) != 0 {
		t.Errorf("superseded = %v, want none", superseded)
	}
}

// The guard that matters most: a lane with no complete stream would make every unit only
// it builds look dead, and sweeping on that would cost a cold build. So the live set is
// unsound, and the sweep leaves every unit alone.
func TestAssembleLiveIsUnsoundWithoutEveryLanesCompleteStream(t *testing.T) {
	base := allComplete(t0)
	cases := []struct {
		name  string
		files []StreamFile
		want  string
	}{
		{"no streams at all", nil, "clippy"},
		{"one lane missing", base[:2], "tests"},
		{"an empty stream", []StreamFile{base[0], base[1], stream("tests.jsonl", t0, "")}, "tests"},
		{"a stream with no units", []StreamFile{base[0], base[1], stream("tests.jsonl", t0, finished)}, "tests"},
		{"an unfinished stream", []StreamFile{base[0], base[1], stream("tests.jsonl", t0, artifactLine("/t/debug/deps/libc-3333333333333333.rlib"))}, "tests"},
		{"a garbled line", []StreamFile{base[0], base[1], stream("tests.jsonl", t0, `{"reason":`, artifactLine("/t/debug/deps/libc-3333333333333333.rlib"), finished)}, "tests"},
		{"only a partial", []StreamFile{base[0], base[1], stream("tests.partial-1-2.jsonl", t0, artifactLine("/t/debug/deps/libc-3333333333333333.rlib"), finished)}, "tests"},
	}
	for _, c := range cases {
		live, _ := assembleLive(c.files, Lanes)
		if live.Problem == "" || !strings.Contains(live.Problem, c.want) {
			t.Errorf("%s: problem = %q, want one naming %q", c.name, live.Problem, c.want)
		}
	}
}

// A run that didn't finish its build (a compile error, a kill) still built units, and its
// lane's complete stream is older than they are. Its partial stream is what vouches for
// them, until a later finished run of the same lane supersedes it.
func TestAssembleLiveCountsPartialsNewerThanTheirLanesCompleteStream(t *testing.T) {
	files := append(allComplete(t0),
		stream("tests.partial-100-1.jsonl", t0.Add(time.Hour), artifactLine("/t/debug/deps/libd-4444444444444444.rlib")),
		stream("tests.partial-50-1.jsonl", t0.Add(-time.Hour), artifactLine("/t/debug/deps/libe-5555555555555555.rlib")),
		// A truncated last line is a killed run's signature; the rest still counts.
		stream("clippy.partial-200-1.jsonl", t0.Add(time.Hour), artifactLine("/t/debug/deps/libf-6666666666666666.rmeta"), `{"reason":"compi`),
	)
	live, superseded := assembleLive(files, Lanes)
	if live.Problem != "" {
		t.Fatalf("unexpected problem: %s", live.Problem)
	}
	for _, h := range []string{"4444444444444444", "6666666666666666"} {
		if !live.Hashes[h] {
			t.Errorf("a partial newer than its lane's complete stream must count: %s missing", h)
		}
	}
	if live.Hashes["5555555555555555"] {
		t.Error("a partial older than its lane's complete stream is superseded and must not count")
	}
	if strings.Join(superseded, ",") != "tests.partial-50-1.jsonl" {
		t.Errorf("superseded = %v, want the older tests partial", superseded)
	}
}

// A stream from a lane this code doesn't know (one that was renamed) only ever adds
// hashes: keeping more is the safe direction.
func TestAssembleLiveKeepsUnknownLanesStreams(t *testing.T) {
	files := append(allComplete(t0), stream("old-lane.jsonl", t0, artifactLine("/t/debug/deps/libz-7777777777777777.rlib"), finished))
	live, _ := assembleLive(files, Lanes)
	if !live.Hashes["7777777777777777"] {
		t.Error("an unknown lane's stream must still count")
	}
}

func sortedKeys(m map[string]bool) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}
