package checks

import (
	"strings"
	"testing"
)

func TestOperationsLocalDiskBoundary(t *testing.T) {
	files := map[string]string{
		"lib.rs": "pub use delete::trash::{move_to_trash_sync};\n#[cfg(test)]\nmod fixture;\n",
		// Shared code: each host-OS name is a violation, a comment naming one isn't.
		"summary.rs":           "// dirs::home_dir is the old way\nfn a() { dirs::home_dir(); }\nuse libc;\nuse std::os::unix::fs::MetadataExt;\n",
		"rename.rs":            "fn b() {}\n#[cfg(all(test, unix))]\nmod tests {\n    fn c() { libc::geteuid(); }\n}\nfn d() { cmdr_local::x(); }\n",
		"fixture.rs":           "fn e() { libc::geteuid(); }\n",
		"local_disk/mod.rs":    "fn f() { libc::access(); }\n",
		"transfer/copy/mod.rs": "fn g() { libc::copyfile(); }\n",
		"validation_test.rs":   "fn h() { libc::geteuid(); }\n",
	}
	violations, scanned := operationsBoundaryViolations(files)
	want := []string{
		"crates/cmdr-operations/src/rename.rs:6: fn d() { cmdr_local::x(); }",
		"crates/cmdr-operations/src/summary.rs:2: fn a() { dirs::home_dir(); }",
		"crates/cmdr-operations/src/summary.rs:3: use libc;",
		"crates/cmdr-operations/src/summary.rs:4: use std::os::unix::fs::MetadataExt;",
	}
	if strings.Join(violations, "\n") != strings.Join(want, "\n") {
		t.Errorf("violations:\n%s\nwant:\n%s", strings.Join(violations, "\n"), strings.Join(want, "\n"))
	}
	if scanned != 3 {
		t.Errorf("scanned %d production files, want 3 (lib.rs, rename.rs, summary.rs)", scanned)
	}
}

func TestOperationsBrowserGate(t *testing.T) {
	files := map[string]string{}
	// Every local-engine module declared and gated: nothing to report.
	for _, module := range operationsLocalDiskModules {
		if module == "local_disk/" {
			continue
		}
		trimmed := strings.TrimSuffix(strings.TrimSuffix(module, "/"), ".rs")
		declarer, name := "lib.rs", trimmed
		if i := strings.LastIndex(trimmed, "/"); i >= 0 {
			declarer, name = trimmed[:i]+"/mod.rs", trimmed[i+1:]
		}
		files[declarer] += "#[cfg(not(target_family = \"wasm\"))]\npub(super) mod " + name + ";\n"
	}
	if got := operationsBrowserGateViolations(files); len(got) != 0 {
		t.Fatalf("a fully gated tree reported %v", got)
	}
	// Seen red: one gate dropped.
	files["lib.rs"] = strings.Replace(files["lib.rs"], "#[cfg(not(target_family = \"wasm\"))]\npub(super) mod scan;\n", "pub(super) mod scan;\n", 1)
	got := operationsBrowserGateViolations(files)
	if len(got) != 1 || !strings.HasPrefix(got[0], "scan.rs:") {
		t.Errorf("violations %v, want only scan.rs", got)
	}
}
