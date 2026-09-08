package checks

import "testing"

// Real `go version -m` output, captured from a tool binary. The first line carries the Go toolchain
// that built it; the `mod` line carries the module and the version it was built from.
const sampleBuildInfo = `/Users/x/go/bin/deadcode: go1.26.0
	path	golang.org/x/tools/cmd/deadcode
	mod	golang.org/x/tools	v0.42.0	h1:uNgphsn75Tdz5Ji2q36v/nsFSfR/9BRFvqhGBaJGd5k=
	dep	golang.org/x/mod	v0.30.0	h1:PiqA/eBaHKM7XLLbSMOJVfa8LFyzykaXbNZ8pXTIRUM=
	build	-buildmode=exe
`

func TestParseGoToolBuildInfoReadsTheVersionsThatDecideAReinstall(t *testing.T) {
	info, ok := parseGoToolBuildInfo(sampleBuildInfo)
	if !ok {
		t.Fatal("expected the sample to parse")
	}
	if info.ModuleVersion != "v0.42.0" {
		t.Errorf("module version = %q, want v0.42.0", info.ModuleVersion)
	}
	if info.GoVersion != "go1.26.0" {
		t.Errorf("go version = %q, want go1.26.0", info.GoVersion)
	}
}

func TestParseGoToolBuildInfoRejectsOutputItCannotRead(t *testing.T) {
	for name, output := range map[string]string{
		"empty":                           "",
		"no mod line":                     "/Users/x/go/bin/tool: go1.27.1\n\tpath\texample.com/tool\n",
		"no go version on the first line": "\tmod\texample.com/tool\tv1.2.3\th1:abc=\n",
		"not a go binary":                 "some other tool: unrecognized file format\n",
	} {
		if _, ok := parseGoToolBuildInfo(output); ok {
			t.Errorf("%s: expected the parse to fail so the caller reinstalls", name)
		}
	}
}

// A pinned install path is what makes a reinstall decidable: without the `@version` suffix there is
// nothing to compare a binary against, and the repo's supply-chain rule forbids the unpinned form
// anyway (`DETAILS.md` § Key decisions).
func TestPinnedModuleVersionSplitsTheInstallPath(t *testing.T) {
	version, err := pinnedModuleVersion("golang.org/x/tools/cmd/deadcode@v0.49.0")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if version != "v0.49.0" {
		t.Errorf("version = %q, want v0.49.0", version)
	}

	pseudo, err := pinnedModuleVersion("go.uber.org/nilaway/cmd/nilaway@v0.0.0-20260808063849-8649a03c818a")
	if err != nil {
		t.Fatalf("unexpected error on a pseudo-version: %v", err)
	}
	if pseudo != "v0.0.0-20260808063849-8649a03c818a" {
		t.Errorf("pseudo-version = %q, want the full pseudo-version", pseudo)
	}

	if _, err := pinnedModuleVersion("golang.org/x/tools/cmd/deadcode"); err == nil {
		t.Error("expected an unpinned install path to be rejected")
	}
	if _, err := pinnedModuleVersion("golang.org/x/tools/cmd/deadcode@latest"); err == nil {
		t.Error("expected `@latest` to be rejected: it defeats the pin")
	}
}
