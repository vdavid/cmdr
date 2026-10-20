package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

const testFollowupPrompt = `---
name: followup
purpose: Opens a follow-up turn.
model: any tool-calling chat model
version: 2026-10-20
variables: decisions
---
The user answered:
{{decisions}}Say thanks.
`

const testSystemPrompt = `---
name: system
purpose: The system prompt.
model: any tool-calling chat model
version: 2026-10-20
variables:
---
You are Ask Cmdr.
`

const testPromptsModule = `
//! Keep every call in the literal render(&CONST, &[("name", value), …]) shape.
/// A doc line naming render(&NOT_A_PROMPT, &[]) is not a call.
const SYSTEM: PromptFile = prompt!("system");
const FOLLOWUP: PromptFile = prompt!("followup");

pub fn system_prompt() -> Cow<'static, str> {
    render(&SYSTEM, &[])
}

pub fn followup(decisions: &str) -> String {
    render(&FOLLOWUP, &[("decisions", decisions)]).into_owned()
}

fn render(file: &PromptFile, vars: &[(&str, &str)]) -> Cow<'static, str> {
    todo!()
}
`

// runPromptLintOn writes the prompt files (name -> body, mod.rs included) into the prompts
// dir of a temp repo and runs the check over it.
func runPromptLintOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	root := t.TempDir()
	dir := filepath.Join(root, filepath.FromSlash(promptsDir))
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	for name, body := range files {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	return RunPromptLint(&CheckContext{RootDir: root})
}

func validPromptFiles() map[string]string {
	return map[string]string{
		"system.md":   testSystemPrompt,
		"followup.md": testFollowupPrompt,
		"mod.rs":      testPromptsModule,
		// Agent docs beside the prompts are not prompts.
		"CLAUDE.md":  "# Prompts\n{{not a template}}\n",
		"DETAILS.md": "# Details\n",
	}
}

func expectPromptLintError(t *testing.T, files map[string]string, wants ...string) {
	t.Helper()
	_, err := runPromptLintOn(t, files)
	if err == nil {
		t.Fatalf("expected a prompt-lint failure mentioning %q, got success", wants)
	}
	for _, want := range wants {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("expected %q in the failure, got:\n%s", want, err.Error())
		}
	}
}

func TestPromptLint_PassesWhenEveryPromptAndCallSiteAgree(t *testing.T) {
	res, err := runPromptLintOn(t, validPromptFiles())
	if err != nil {
		t.Fatalf("expected success, got: %v", err)
	}
	if res.Code != ResultSuccess || !strings.Contains(res.Message, "2 prompts") {
		t.Errorf("expected a success naming 2 prompts, got: %+v", res)
	}
}

// The bug the check exists for: a typo'd variable the call site never provides reaches the
// model verbatim.
func TestPromptLint_FlagsAVariableTheFrontmatterDoesNotDeclare(t *testing.T) {
	files := validPromptFiles()
	files["followup.md"] = strings.Replace(testFollowupPrompt, "{{decisions}}", "{{decisions}}{{folder_digest}}", 1)
	expectPromptLintError(t, files, "followup.md", "folder_digest")
}

func TestPromptLint_FlagsADeclaredVariableTheBodyNeverUses(t *testing.T) {
	files := validPromptFiles()
	files["system.md"] = strings.Replace(testSystemPrompt, "variables:\n", "variables: unused\n", 1)
	expectPromptLintError(t, files, "system.md", "unused")
}

func TestPromptLint_FlagsATemplateThatDoesNotCompile(t *testing.T) {
	for name, body := range map[string]string{
		"unclosed":   "Hello {{decisions\n",
		"stray":      "Hello decisions}}\n",
		"not_a_name": "Hello {{two words}}\n",
	} {
		t.Run(name, func(t *testing.T) {
			files := validPromptFiles()
			files["followup.md"] = strings.SplitAfter(testFollowupPrompt, "---\n")[0] +
				strings.SplitAfter(testFollowupPrompt, "---\n")[1] + body
			expectPromptLintError(t, files, "followup.md")
		})
	}
}

func TestPromptLint_FlagsIncompleteOrUnknownFrontmatter(t *testing.T) {
	files := validPromptFiles()
	files["system.md"] = strings.Replace(testSystemPrompt, "model: any tool-calling chat model\n", "", 1)
	expectPromptLintError(t, files, "system.md", "model")

	files = validPromptFiles()
	files["system.md"] = strings.Replace(testSystemPrompt, "name: system", "name: sys", 1)
	expectPromptLintError(t, files, "system.md", "name")

	files = validPromptFiles()
	files["system.md"] = strings.Replace(testSystemPrompt, "version: 2026-10-20\n", "version: 2026-10-20\nauthor: me\n", 1)
	expectPromptLintError(t, files, "system.md", "author")

	files = validPromptFiles()
	files["system.md"] = "You are Ask Cmdr.\n"
	expectPromptLintError(t, files, "system.md", "frontmatter")
}

func TestPromptLint_FlagsACallSiteWhoseVariablesDiffer(t *testing.T) {
	files := validPromptFiles()
	files["mod.rs"] = strings.Replace(testPromptsModule, `&[("decisions", decisions)]`, `&[("decision", decisions)]`, 1)
	expectPromptLintError(t, files, "followup", "decision")

	files = validPromptFiles()
	files["mod.rs"] = strings.Replace(testPromptsModule, `render(&SYSTEM, &[])`, `render(&SYSTEM, &[("extra", "x")])`, 1)
	expectPromptLintError(t, files, "system", "extra")
}

func TestPromptLint_FlagsAPromptFileWithNoDeclarationOrNoCallSite(t *testing.T) {
	files := validPromptFiles()
	files["orphan.md"] = strings.Replace(testSystemPrompt, "name: system", "name: orphan", 1)
	expectPromptLintError(t, files, "orphan.md")

	files = validPromptFiles()
	files["mod.rs"] = strings.Replace(testPromptsModule, "render(&SYSTEM, &[])", "todo!()", 1)
	expectPromptLintError(t, files, "SYSTEM")
}

// The lint reads call sites by their literal shape, so a call it can't read fails closed rather
// than passing unchecked.
func TestPromptLint_FailsClosedOnACallItCannotRead(t *testing.T) {
	files := validPromptFiles()
	files["mod.rs"] = strings.Replace(testPromptsModule, `render(&FOLLOWUP, &[("decisions", decisions)])`, `render(&FOLLOWUP, vars)`, 1)
	expectPromptLintError(t, files, "render")
}
