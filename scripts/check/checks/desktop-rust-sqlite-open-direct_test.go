package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// runSqliteOpenDirectOn writes the supplied files into a temp repo layout rooted
// at `apps/desktop/src-tauri/src/` and runs the check over them.
func runSqliteOpenDirectOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	for rel, body := range files {
		// A key starting with `crates/` is repo-root-relative, so a test can place
		// the factory file itself (which lives in `cmdr-fs`, not the app).
		full := filepath.Join(srcDir, rel)
		if strings.HasPrefix(rel, "crates/") {
			full = filepath.Join(root, rel)
		}
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	return RunSqliteOpenDirect(&CheckContext{RootDir: root})
}

func TestSqliteOpenDirect_FlagsEveryOpenFlavor(t *testing.T) {
	for name, body := range map[string]string{
		"plain":      "fn f() { let c = Connection::open(&path)?; }\n",
		"qualified":  "fn f() { let c = rusqlite::Connection::open(&path)?; }\n",
		"with_flags": "fn f() { let c = Connection::open_with_flags(&path, flags)?; }\n",
		"in_memory":  "fn f() { let c = Connection::open_in_memory()?; }\n",
	} {
		t.Run(name, func(t *testing.T) {
			_, err := runSqliteOpenDirectOn(t, map[string]string{"store/connection.rs": body})
			if err == nil {
				t.Fatal("expected a violation for a direct Connection::open*, got success")
			}
			if !strings.Contains(err.Error(), "store/connection.rs") {
				t.Errorf("expected the offending path in the message, got: %s", err.Error())
			}
		})
	}
}

func TestSqliteOpenDirect_AllowsTheFactoryItself(t *testing.T) {
	res, err := runSqliteOpenDirectOn(t, map[string]string{
		"crates/cmdr-fs/src/sqlite_util.rs": "fn open(p: &Path) -> Result<Connection> { Connection::open(p) }\n",
	})
	if err != nil {
		t.Fatalf("the factory file must be allowed to open connections, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestSqliteOpenDirect_AllowsFactoryCallsAndComments(t *testing.T) {
	res, err := runSqliteOpenDirectOn(t, map[string]string{
		"store/connection.rs": "" +
			"/// ❌ Don't call `Connection::open(path)` directly.\n" +
			"fn f() { let c = crate::sqlite_util::open(&path)?; }\n" +
			"fn g() { let c = crate::sqlite_util::open_read_only(&path)?; }\n",
	})
	if err != nil {
		t.Fatalf("factory calls and doc comments must pass, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}
