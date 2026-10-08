package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestMiseGoVersionReadsThePin(t *testing.T) {
	root := repoRootForTest(t)
	version, err := MiseGoVersion(root)
	if err != nil {
		t.Fatalf("MiseGoVersion: %v", err)
	}
	if !strings.HasPrefix(version, "1.") {
		t.Fatalf("expected a Go 1.x version from .mise.toml, got %q", version)
	}
	// The whole point of the helper is that it agrees with the file, so assert
	// against the raw bytes rather than a hardcoded version that would need
	// editing on every Renovate bump.
	raw := readRepoFile(t, root, ".mise.toml")
	if !strings.Contains(raw, `go = "`+version+`"`) {
		t.Fatalf("MiseGoVersion returned %q, which is not the `go = \"...\"` line in .mise.toml", version)
	}
}

func TestMiseGoVersionFailsLoudlyWithoutAPin(t *testing.T) {
	dir := t.TempDir()
	writeFiles(t, dir, map[string]string{".mise.toml": "[tools]\nnode = \"26\"\n"})
	if _, err := MiseGoVersion(dir); err == nil {
		t.Fatal("expected an error when .mise.toml pins no Go version")
	}
}

// There's deliberately no test running the check against the real tree. The
// registered `go-version-single-source` check IS that guard: it declares
// `wholeRepoInputs`, so it re-runs on any edit that could introduce a pin, and
// CI runs it on every push. A Go test doing the same would force the
// `scripts-go-tests` lane to fingerprint the whole repo (see
// `realTreeReadingTests`), costing every Go lint a cache miss per repo change to
// duplicate a check that already runs.

// `git ls-files` keeps listing a tracked file until its deletion is STAGED, so
// between `rm` and `git add` the scan gets a path with nothing behind it. That
// deletion says nothing about the pin invariant, so the lane skips the file and
// stays green: an agent deleting a locale JSON must not see `pnpm check --fast`
// go red with `open apps/.../servers.json: no such file or directory`.
func TestGoVersionSkipsATrackedFileDeletedLocally(t *testing.T) {
	dir := t.TempDir()
	gitInit(t, dir)
	writeFiles(t, dir, map[string]string{
		".mise.toml":  "[tools]\ngo = \"1.27.0\"\n",
		"kept.json":   "{\"name\": \"kept\"}\n",
		"doomed.json": "{\"name\": \"doomed\"}\n",
	})
	gitRun(t, dir, "add", ".mise.toml", "kept.json", "doomed.json")
	if err := os.Remove(filepath.Join(dir, "doomed.json")); err != nil {
		t.Fatal(err)
	}

	result, err := RunGoVersionSingleSource(&CheckContext{RootDir: dir})
	if err != nil {
		t.Fatalf("an unstaged deletion must not fail the lane: %v", err)
	}
	if strings.Contains(result.Message, "doomed.json") {
		t.Errorf("the skip is silent; a deleted file has nothing to report, got: %s", result.Message)
	}
	// The file that's still there is still scanned, so the skip can't quietly
	// turn into "scan nothing".
	if result.Total != 1 {
		t.Errorf("expected the 1 surviving file to be scanned, got %d (%s)", result.Total, result.Message)
	}
}

// Same for a `go.mod`: its floor comes from a separate read, with the same hole.
func TestGoVersionSkipsAGoModDeletedLocally(t *testing.T) {
	dir := t.TempDir()
	gitInit(t, dir)
	writeFiles(t, dir, map[string]string{
		".mise.toml": "[tools]\ngo = \"1.27.0\"\n",
		"a/go.mod":   "module a\n\ngo 1.25\n",
		"b/go.mod":   "module b\n\ngo 1.25\n",
	})
	gitRun(t, dir, "add", ".mise.toml", "a/go.mod", "b/go.mod")
	if err := os.Remove(filepath.Join(dir, "b", "go.mod")); err != nil {
		t.Fatal(err)
	}

	result, err := RunGoVersionSingleSource(&CheckContext{RootDir: dir})
	if err != nil {
		t.Fatalf("an unstaged `go.mod` deletion must not fail the lane: %v", err)
	}
	if !strings.Contains(result.Message, "1 go.mod on floor 1.25") {
		t.Errorf("expected only the surviving module to count toward the floor, got: %s", result.Message)
	}
}

func TestGoVersionScannerCatchesEveryPinShape(t *testing.T) {
	cases := []struct {
		name string
		file string
		line string
	}{
		{"go const", "x.go", `const goVersion = "1.27.0"`},
		{"workflow input", "ci.yml", `          go-version: '1.27'`},
		{"dockerfile arg", "Dockerfile", `ARG GO_VERSION=1.27.0`},
		{"shell export", "x.sh", `GO_VERSION=1.27`},
		{"download url", "x.sh", `curl -fsSL https://go.dev/dl/go1.27.0.linux-amd64.tar.gz`},
		{"docker image", "Dockerfile", `FROM golang:1.27`},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			violations, _ := scanForGoVersionPins([]byte(tc.line+"\n"), tc.file)
			if len(violations) != 1 {
				t.Fatalf("expected 1 violation for %q, got %d", tc.line, len(violations))
			}
		})
	}
}

func TestGoVersionScannerLeavesInnocentLinesAlone(t *testing.T) {
	// The format-placeholder line is the shape the fixed container provisioner
	// produces, so a match here would make the correct code unwritable.
	innocent := []string{
		`curl -fsSL https://go.dev/dl/go%[1]s.linux-${ARCH}.tar.gz | tar -xz -C /usr/local`,
		`goVersion, err := MiseGoVersion(ctx.RootDir)`,
		`"version": "1.27.0"`,
		`node = "26"`,
		`golang.org/x/tools/cmd/deadcode@v0.49.0`,
	}
	for _, line := range innocent {
		violations, _ := scanForGoVersionPins([]byte(line+"\n"), "x.go")
		if len(violations) != 0 {
			t.Errorf("false positive on %q:\n%s", line, strings.Join(violations, "\n"))
		}
	}
}

// Prose naming a version is not a pin. Without this, the doc comments
// explaining the check would trip the check.
func TestGoVersionScannerSkipsWholeLineComments(t *testing.T) {
	for _, line := range []string{
		`# the old GO_VERSION=1.25.7 const lived here`,
		`// a workflow's go-version: '1.27' would trip this`,
		`    # FROM golang:1.27 was replaced by the mise install`,
	} {
		violations, _ := scanForGoVersionPins([]byte(line+"\n"), "x.sh")
		if len(violations) != 0 {
			t.Errorf("a comment is not a pin, but %q was flagged", line)
		}
	}
}

func TestGoVersionScannerHonoursTheOptOut(t *testing.T) {
	violations, orphans := scanForGoVersionPins(
		[]byte("GO_VERSION=1.27 # allowed-go-version-pin: upstream installer wants it inline\n"), "x.sh")
	if len(violations) != 0 {
		t.Fatalf("opt-out ignored: %s", strings.Join(violations, "\n"))
	}
	if len(orphans) != 0 {
		t.Fatalf("a used directive must not be reported as an orphan")
	}
}

func TestGoVersionScannerRejectsAReasonlessOptOut(t *testing.T) {
	violations, _ := scanForGoVersionPins([]byte("GO_VERSION=1.27 # allowed-go-version-pin:\n"), "x.sh")
	if len(violations) != 1 {
		t.Fatalf("an empty reason must not excuse the pin, got %d violations", len(violations))
	}
}

func TestGoModFloorsMustAgreeWithEachOther(t *testing.T) {
	floors := []goModFloor{
		{relPath: "a/go.mod", version: "1.24"},
		{relPath: "b/go.mod", version: "1.25"},
	}
	msg := checkGoModFloors(floors, "1.27.0")
	if msg == "" {
		t.Fatal("expected disagreeing floors to be reported")
	}
	if !strings.Contains(msg, "a/go.mod") || !strings.Contains(msg, "b/go.mod") {
		t.Fatalf("the report must name both files, got:\n%s", msg)
	}
}

// `go mod tidy` rewrites `go 1.25` to `go 1.25.0` on a module whose deps ask for
// it, so the two spellings must count as one floor. Enforcing the literal text
// would make the check and Go's own tooling rewrite each other forever.
func TestGoModFloorSpellingsOfOneVersionAgree(t *testing.T) {
	floors := []goModFloor{
		{relPath: "a/go.mod", version: "1.25"},
		{relPath: "b/go.mod", version: "1.25.0"},
	}
	if msg := checkGoModFloors(floors, "1.27.0"); msg != "" {
		t.Fatalf("`1.25` and `1.25.0` are the same floor, got:\n%s", msg)
	}
}

func TestGoModFloorAboveMiseIsRejected(t *testing.T) {
	floors := []goModFloor{{relPath: "a/go.mod", version: "1.28"}}
	msg := checkGoModFloors(floors, "1.27.0")
	if msg == "" {
		t.Fatal("a floor above the pinned toolchain must be reported: GOTOOLCHAIN=auto would download it")
	}
}

// A floor BELOW mise is the steady state, and it's what keeps a Renovate
// toolchain bump from ever turning this check red.
func TestGoModFloorBelowMiseIsFine(t *testing.T) {
	floors := []goModFloor{
		{relPath: "a/go.mod", version: "1.25"},
		{relPath: "b/go.mod", version: "1.25"},
	}
	if msg := checkGoModFloors(floors, "1.27.0"); msg != "" {
		t.Fatalf("a lagging floor is legal, got:\n%s", msg)
	}
}

func TestCompareGoVersionsIsNumericNotLexical(t *testing.T) {
	cases := []struct {
		a, b string
		want int
	}{
		{"1.25", "1.25.0", 0},
		{"1.9", "1.27", -1}, // lexically "1.9" > "1.27"; numerically it isn't
		{"1.27.1", "1.27.0", 1},
		{"1.27", "1.27.0", 0},
		{"1.26.9", "1.27", -1},
	}
	for _, tc := range cases {
		if got := compareGoVersions(tc.a, tc.b); got != tc.want {
			t.Errorf("compareGoVersions(%q, %q) = %d, want %d", tc.a, tc.b, got, tc.want)
		}
	}
}

// The container provisioner must derive its Go from .mise.toml. A literal here
// is exactly the drift that started this: the const said 1.25.7 while mise had
// moved to 1.27.0.
func TestLinuxContainerProvisionsTheMisePinnedGo(t *testing.T) {
	root := repoRootForTest(t)
	version, err := MiseGoVersion(root)
	if err != nil {
		t.Fatalf("MiseGoVersion: %v", err)
	}
	dockerfile := repoLinuxDockerfile(t, root)
	if !strings.Contains(dockerfile, "go"+version+".linux-") {
		t.Fatalf("the Linux lanes' image does not download Go %s from .mise.toml", version)
	}
}
