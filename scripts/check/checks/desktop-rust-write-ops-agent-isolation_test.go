package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// runWriteOpsAgentIsolationOn writes the supplied files into a temp repo layout
// under the write-engine subtree and runs the check over them.
func runWriteOpsAgentIsolationOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	engineDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src", "file_system", "write_operations")
	for rel, fileBody := range files {
		full := filepath.Join(engineDir, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(fileBody), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	return RunWriteOpsAgentIsolation(&CheckContext{RootDir: root})
}

func TestWriteOpsAgentIsolation_FlagsEveryWayIn(t *testing.T) {
	for name, fileBody := range map[string]string{
		"use_crate":  "use crate::agent::store::proposals;\n",
		"call_site":  "fn f() { crate::agent::store::mark_done(id); }\n",
		"super_path": "fn f() { super::super::agent::types::Thing::new(); }\n",
		"bare_path":  "fn f() { let s = agent::store::open(); }\n",
	} {
		t.Run(name, func(t *testing.T) {
			_, err := runWriteOpsAgentIsolationOn(t, map[string]string{"mod.rs": fileBody})
			if err == nil {
				t.Fatal("expected a violation for the engine naming the agent module, got success")
			}
			if !strings.Contains(err.Error(), "mod.rs") {
				t.Errorf("expected the offending path in the message, got: %s", err.Error())
			}
		})
	}
}

// The engine docs discuss the agent constantly: naming the module you must not
// call is how you explain the boundary. Only code may not reach it.
func TestWriteOpsAgentIsolation_AllowsProse(t *testing.T) {
	res, err := runWriteOpsAgentIsolationOn(t, map[string]string{
		"source_binding.rs": "" +
			"//! The engine must never reach into agent::store to record an outcome;\n" +
			"//! the injected sink is the seam.\n" +
			"/// A caller wanting agent::store::ProposalStatus written wraps the sink.\n" +
			"    // even indented, agent::whatever in a comment is prose\n" +
			"pub(crate) struct ExpectedSources {}\n",
	})
	if err != nil {
		t.Fatalf("comments naming the agent module must pass, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

// A word that merely ENDS in agent is a different identifier, and flagging it
// would push authors toward renaming their own types to appease a scanner.
func TestWriteOpsAgentIsolation_AllowsIdentifiersEndingInAgent(t *testing.T) {
	res, err := runWriteOpsAgentIsolationOn(t, map[string]string{
		"transfer/http.rs": "fn f() { let ua = user_agent::current(); }\n",
	})
	if err != nil {
		t.Fatalf("user_agent is not the agent module, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

// A test reaching for the agent proves the engine knows about it, which is the
// thing being forbidden, so the fence covers tests too.
func TestWriteOpsAgentIsolation_CoversTestsToo(t *testing.T) {
	_, err := runWriteOpsAgentIsolationOn(t, map[string]string{
		"source_binding/tests.rs": "use crate::agent::store::proposals::ProposalStatus;\n",
	})
	if err == nil {
		t.Fatal("expected a violation from a test file, got success")
	}
	if !strings.Contains(err.Error(), "tests.rs") {
		t.Errorf("expected the offending test path in the message, got: %s", err.Error())
	}
}

func TestWriteOpsAgentIsolation_PassesOnAnEngineThatMindsItsOwnBusiness(t *testing.T) {
	res, err := runWriteOpsAgentIsolationOn(t, map[string]string{
		"mod.rs":            "pub async fn copy_files_start() {}\n",
		"source_binding.rs": "pub(crate) fn retain_bound_sources() {}\n",
	})
	if err != nil {
		t.Fatalf("a clean engine must pass, got: %v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
	if !strings.Contains(res.Message, "knows nothing about the agent") {
		t.Errorf("expected the success message to say what held, got: %s", res.Message)
	}
}
