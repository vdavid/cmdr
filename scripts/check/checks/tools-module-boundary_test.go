package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func runToolsBoundaryOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	baseline := map[string]string{
		"tools/mod.rs": "pub mod read;\n",
		"mcp/mod.rs":   "mod server;\n",
	}
	for rel, body := range files {
		baseline[rel] = body
	}
	for rel, body := range baseline {
		full := filepath.Join(srcDir, rel)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	return RunToolsModuleBoundary(&CheckContext{RootDir: root})
}

func TestToolsBoundary_FlagsAnAgentImportInMcp(t *testing.T) {
	_, err := runToolsBoundaryOn(t, map[string]string{
		"mcp/server.rs": "use crate::agent::types::TurnOrigin;\n",
	})
	if err == nil || !strings.Contains(err.Error(), "mcp/server.rs:1") {
		t.Fatalf("expected a violation at mcp/server.rs:1, got %v", err)
	}
}

func TestToolsBoundary_FlagsEvenAnAllowedAgentModuleInMcp(t *testing.T) {
	_, err := runToolsBoundaryOn(t, map[string]string{
		"mcp/auth.rs": "fn f() { crate::agent::store::open_read_connection(); }\n",
	})
	if err == nil {
		t.Fatal("expected a violation: `mcp` may name no `agent` module at all")
	}
}

func TestToolsBoundary_FlagsAnUnlistedAgentModuleInTools(t *testing.T) {
	for name, source := range map[string]string{
		"path":        "use crate::agent::wake::job;\n",
		"bare module": "use crate::agent;\n",
		"llm sibling": "use crate::agent::llm::fake::FakeAgentLlm;\n",
		"chat ":       "use crate::agent::chat::runtime::ChatRuntime;\n",
		"brace group": "use crate::agent::{store::x, wake::y};\n",
		"multi-line":  "use crate::agent::{\n    types::A,\n    outcomes::B,\n};\n",
		"nested":      "use crate::agent::llm::{types::A, fake::B};\n",
		"root group":  "use crate::{agent, location};\n",
		"relative":    "use super::super::agent::wake::job;\n",
	} {
		t.Run(name, func(t *testing.T) {
			_, err := runToolsBoundaryOn(t, map[string]string{"tools/read/a.rs": source})
			if err == nil {
				t.Fatalf("expected a violation for %q", source)
			}
		})
	}
}

func TestToolsBoundary_AcceptsEveryListedAgentImportInTools(t *testing.T) {
	for _, source := range []string{
		"use crate::agent::store::proposals::OpSelection;\n",
		"use crate::agent::suggested_ops::facts::RunsAs;\n",
		"use crate::agent::memory::MemoryStore;\n",
		"use crate::agent::chat::budget::{MAX_TOOL_RESULT_TOKENS, estimate_serialized_tokens};\n",
		"use crate::agent::types::{OpStatus, ProposalStatus};\n",
		"use crate::agent::llm::types::{ToolDeclaration, ToolId};\n",
		"use crate::agent::AgentDb;\n",
		"use crate::agent::{AgentDb, store::x, types as t};\n",
		"use crate::agent::store::{self, MIGRATIONS};\n",
	} {
		result, err := runToolsBoundaryOn(t, map[string]string{"tools/read/a.rs": source})
		if err != nil {
			t.Fatalf("%q should pass, got %v", source, err)
		}
		if !strings.Contains(result.Message, "boundary holds") {
			t.Fatalf("unexpected message %q", result.Message)
		}
	}
}

func TestToolsBoundary_FlagsATransportImportInTools(t *testing.T) {
	for _, source := range []string{
		"use crate::mcp::config::DEFAULT_PORT;\n",
		"use crate::{location, mcp};\n",
	} {
		if _, err := runToolsBoundaryOn(t, map[string]string{"tools/a.rs": source}); err == nil {
			t.Fatalf("expected a violation for %q", source)
		}
	}
}

func TestToolsBoundary_LetsMcpNameItselfAndIgnoresComments(t *testing.T) {
	_, err := runToolsBoundaryOn(t, map[string]string{
		"mcp/tests/a.rs":   "use crate::mcp::protocol::McpRequest;\n",
		"mcp/server.rs":    "// See `crate::agent::tool_gate`.\nuse crate::tools::get_all_tools;\n",
		"tools/actions.rs": "/// Reached from `crate::agent::wake`, and `crate::mcp::server` carries it.\nfn f() {}\n",
	})
	if err != nil {
		t.Fatalf("expected a pass, got %v", err)
	}
}

func TestToolsBoundary_ReportsTheLineAfterAComment(t *testing.T) {
	_, err := runToolsBoundaryOn(t, map[string]string{
		"mcp/server.rs": "// a comment\n\nuse crate::agent::types::TurnOrigin;\n",
	})
	if err == nil || !strings.Contains(err.Error(), "mcp/server.rs:3") {
		t.Fatalf("expected a violation at line 3, got %v", err)
	}
}

func TestToolsBoundary_FailsWhenAModuleHasMovedAway(t *testing.T) {
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	mustWrite(t, filepath.Join(root, "apps", "desktop", "src-tauri", "src", "mcp", "mod.rs"), "mod server;\n")
	if _, err := RunToolsModuleBoundary(&CheckContext{RootDir: root}); err == nil {
		t.Fatal("expected a failure when `tools/` isn't there to scan")
	}
}
