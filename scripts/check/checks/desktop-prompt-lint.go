package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"sort"
	"strings"
)

// promptsDir holds the agent's prompt files (`<name>.md`, YAML frontmatter plus a
// `{{variable}}` body) and `mod.rs`, the one module that renders them. How the two halves
// work: `apps/desktop/src-tauri/src/agent/prompts/CLAUDE.md`.
const promptsDir = "apps/desktop/src-tauri/src/agent/prompts"

// promptFrontmatterKeys is every key a prompt's frontmatter carries, each required. The
// first four describe the prompt for its next editor; `variables` is the contract with the
// call site.
var promptFrontmatterKeys = []string{"name", "purpose", "model", "version", "variables"}

var (
	promptNameRe = regexp.MustCompile(`^[a-z][a-z0-9_]*$`)
	// promptDeclRe matches `const SYSTEM: PromptFile = prompt!("system");`.
	promptDeclRe = regexp.MustCompile(`const\s+([A-Z][A-Z0-9_]*)\s*:\s*PromptFile\s*=\s*prompt!\(\s*"([^"]+)"\s*\)`)
	// promptRenderRe matches a call in the literal shape the lint can read:
	// `render(&FOLLOWUP, &[("decisions", decisions), …])`.
	promptRenderRe = regexp.MustCompile(`(?s)\brender\(\s*&([A-Z][A-Z0-9_]*)\s*,\s*&\[(.*?)\]\s*\)`)
	// promptRenderAnyRe counts every `render(` that isn't the function's own definition, so a
	// call in any other shape fails closed instead of going unchecked.
	promptRenderAnyRe = regexp.MustCompile(`\brender\(`)
	promptRenderDefRe = regexp.MustCompile(`\bfn\s+render\(`)
	promptVarKeyRe    = regexp.MustCompile(`\(\s*"([^"]*)"\s*,`)
)

// promptFile is one parsed prompt.
type promptFile struct {
	file      string // base name, for messages
	variables map[string]bool
}

// RunPromptLint checks that every agent prompt is well-formed and that its call site
// provides exactly the variables it uses. It catches the bug templates otherwise ship
// silently: a `{{folder_digest}}` typo sent to the model verbatim.
func RunPromptLint(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, filepath.FromSlash(promptsDir))
	entries, err := os.ReadDir(dir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read %s: %w", promptsDir, err)
	}

	var problems []string
	prompts := map[string]promptFile{} // by frontmatter name
	for _, entry := range entries {
		name := entry.Name()
		if entry.IsDir() || !strings.HasSuffix(name, ".md") || name == "CLAUDE.md" || name == "DETAILS.md" {
			continue
		}
		raw, readErr := os.ReadFile(filepath.Join(dir, name))
		if readErr != nil {
			return CheckResult{}, fmt.Errorf("couldn't read %s: %w", name, readErr)
		}
		parsed, fileProblems := parsePromptFile(name, string(raw))
		problems = append(problems, fileProblems...)
		if parsed != nil {
			prompts[strings.TrimSuffix(name, ".md")] = *parsed
		}
	}

	module, err := os.ReadFile(filepath.Join(dir, "mod.rs"))
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read %s/mod.rs: %w", promptsDir, err)
	}
	problems = append(problems, checkPromptCallSites(stripRustLineComments(string(module)), prompts, entries)...)

	if len(problems) > 0 {
		sort.Strings(problems)
		return CheckResult{}, fmt.Errorf(
			"%d %s in the agent's prompts (%s); each prompt's body, its frontmatter `variables:`, and its `render` call in mod.rs must agree:\n%s",
			len(problems), Pluralize(len(problems), "problem", "problems"), promptsDir,
			indentOutput(strings.Join(problems, "\n")),
		)
	}
	return Success(fmt.Sprintf(
		"%d %s checked, every variable declared and provided",
		len(prompts), Pluralize(len(prompts), "prompt", "prompts"),
	)), nil
}

// parsePromptFile validates one prompt's frontmatter and template. It returns the parsed
// prompt when the frontmatter was readable enough to check its call site against.
func parsePromptFile(file, raw string) (*promptFile, []string) {
	rest, ok := strings.CutPrefix(raw, "---\n")
	end := strings.Index(rest, "\n---\n")
	if !ok || end < 0 {
		return nil, []string{file + ": no YAML frontmatter (a `---` line, the keys, a closing `---` line)"}
	}
	header, body := rest[:end], rest[end+len("\n---\n"):]

	values, problems := parsePromptFrontmatter(file, header)
	declared, variableProblems := declaredPromptVariables(file, values["variables"])
	problems = append(problems, variableProblems...)

	used, templateProblems := promptTemplateVariables(body)
	for _, problem := range templateProblems {
		problems = append(problems, file+": "+problem)
	}
	for _, variable := range promptNamesMissing(used, declared) {
		problems = append(problems, fmt.Sprintf("%s: body uses {{%s}}, which `variables:` doesn't declare", file, variable))
	}
	for _, variable := range promptNamesMissing(declared, used) {
		problems = append(problems, fmt.Sprintf("%s: `variables:` declares %q, which the body never uses", file, variable))
	}
	return &promptFile{file: file, variables: declared}, problems
}

// parsePromptFrontmatter reads the `key: value` lines and checks that every key is known,
// present, non-empty (`variables` may be), and that `name` is the file's own stem.
func parsePromptFrontmatter(file, header string) (map[string]string, []string) {
	var problems []string
	values := map[string]string{}
	for _, line := range strings.Split(header, "\n") {
		key, value, found := strings.Cut(line, ":")
		key = strings.TrimSpace(key)
		switch {
		case !found || key == "":
			problems = append(problems, fmt.Sprintf("%s: frontmatter line %q isn't `key: value`", file, line))
		case !slices.Contains(promptFrontmatterKeys, key):
			problems = append(problems, fmt.Sprintf("%s: unknown frontmatter key %q (known: %s)", file, key, strings.Join(promptFrontmatterKeys, ", ")))
		default:
			values[key] = strings.TrimSpace(value)
		}
	}
	for _, key := range promptFrontmatterKeys {
		value, present := values[key]
		if !present {
			problems = append(problems, fmt.Sprintf("%s: frontmatter is missing %q", file, key))
		} else if value == "" && key != "variables" {
			problems = append(problems, fmt.Sprintf("%s: frontmatter %q is empty", file, key))
		}
	}
	stem := strings.TrimSuffix(file, ".md")
	if name, present := values["name"]; present && name != stem {
		problems = append(problems, fmt.Sprintf("%s: frontmatter name %q must match the file name %q", file, name, stem))
	}
	return values, problems
}

// declaredPromptVariables splits the comma-separated `variables:` value.
func declaredPromptVariables(file, list string) (map[string]bool, []string) {
	var problems []string
	declared := map[string]bool{}
	for _, variable := range strings.Split(list, ",") {
		variable = strings.TrimSpace(variable)
		if variable == "" {
			continue
		}
		if !promptNameRe.MatchString(variable) {
			problems = append(problems, fmt.Sprintf("%s: variable %q isn't a lowercase identifier", file, variable))
		}
		declared[variable] = true
	}
	return declared, problems
}

// promptNamesMissing lists the names in `have` that `want` lacks, sorted.
func promptNamesMissing(have, want map[string]bool) []string {
	var missing []string
	for name := range have {
		if !want[name] {
			missing = append(missing, name)
		}
	}
	sort.Strings(missing)
	return missing
}

// promptTemplateVariables compiles a body: every `{{` closes, holds one identifier, and no
// `}}` stands alone.
func promptTemplateVariables(body string) (map[string]bool, []string) {
	used := map[string]bool{}
	var problems []string
	rest := body
	for {
		open := strings.Index(rest, "{{")
		if stray := strings.Index(rest, "}}"); stray >= 0 && (open < 0 || stray < open) {
			problems = append(problems, "a `}}` with no `{{` before it")
			rest = rest[stray+2:]
			continue
		}
		if open < 0 {
			break
		}
		after := rest[open+2:]
		closing := strings.Index(after, "}}")
		if closing < 0 {
			problems = append(problems, "a `{{` that never closes")
			break
		}
		name := strings.TrimSpace(after[:closing])
		if !promptNameRe.MatchString(name) {
			problems = append(problems, fmt.Sprintf("{{%s}} isn't a variable name (lowercase letters, digits, underscores)", after[:closing]))
		} else {
			used[name] = true
		}
		rest = after[closing+2:]
	}
	return used, problems
}

// checkPromptCallSites ties each prompt file to its `prompt!` declaration and its `render`
// call in mod.rs, and compares the call's variable names with the file's.
func checkPromptCallSites(module string, prompts map[string]promptFile, entries []os.DirEntry) []string {
	constToPrompt, problems := checkPromptDeclarations(module, prompts, entries)

	calls := promptRenderRe.FindAllStringSubmatch(module, -1)
	readable := len(promptRenderAnyRe.FindAllString(module, -1)) - len(promptRenderDefRe.FindAllString(module, -1))
	if len(calls) != readable {
		problems = append(problems, fmt.Sprintf(
			"mod.rs: %d `render(` %s, but only %d in the literal `render(&CONST, &[(\"name\", value), …])` shape the lint can read",
			readable, Pluralize(readable, "call", "calls"), len(calls)))
	}
	rendered := map[string]bool{}
	for _, call := range calls {
		constant := call[1]
		name, known := constToPrompt[constant]
		if !known {
			problems = append(problems, fmt.Sprintf("mod.rs: render(&%s, …) names no `prompt!` declaration", constant))
			continue
		}
		rendered[constant] = true
		if prompt, exists := prompts[name]; exists {
			problems = append(problems, checkRenderVariables(constant, call[2], prompt)...)
		}
	}
	for constant := range constToPrompt {
		if !rendered[constant] {
			problems = append(problems, fmt.Sprintf("mod.rs: %s is declared but never rendered", constant))
		}
	}
	return problems
}

// checkPromptDeclarations maps each `prompt!` constant to its prompt, and flags a
// declaration with no file or a file with no declaration.
func checkPromptDeclarations(module string, prompts map[string]promptFile, entries []os.DirEntry) (map[string]string, []string) {
	var problems []string
	constToPrompt := map[string]string{}
	declared := map[string]bool{}
	for _, match := range promptDeclRe.FindAllStringSubmatch(module, -1) {
		constToPrompt[match[1]] = match[2]
		declared[match[2]] = true
		if _, exists := prompts[match[2]]; !exists {
			problems = append(problems, fmt.Sprintf("mod.rs: %s names prompt %q, which has no readable %s.md", match[1], match[2], match[2]))
		}
	}
	for _, entry := range entries {
		stem := strings.TrimSuffix(entry.Name(), ".md")
		if _, isPrompt := prompts[stem]; isPrompt && !declared[stem] {
			problems = append(problems, fmt.Sprintf("%s: no `prompt!(%q)` declaration in mod.rs renders it", entry.Name(), stem))
		}
	}
	return constToPrompt, problems
}

// checkRenderVariables compares the names one `render` call passes with its prompt's.
func checkRenderVariables(constant, slice string, prompt promptFile) []string {
	var problems []string
	provided := map[string]bool{}
	for _, key := range promptVarKeyRe.FindAllStringSubmatch(slice, -1) {
		provided[key[1]] = true
	}
	for _, variable := range promptNamesMissing(prompt.variables, provided) {
		problems = append(problems, fmt.Sprintf("mod.rs: render(&%s, …) doesn't provide %q, which %s uses", constant, variable, prompt.file))
	}
	for _, variable := range promptNamesMissing(provided, prompt.variables) {
		problems = append(problems, fmt.Sprintf("mod.rs: render(&%s, …) provides %q, which %s doesn't declare", constant, variable, prompt.file))
	}
	return problems
}

// stripRustLineComments drops every `//` comment, so the module's own docs (which show the
// call shape) aren't read as calls. mod.rs holds no string containing `//`.
func stripRustLineComments(source string) string {
	lines := strings.Split(source, "\n")
	for i, line := range lines {
		if at := strings.Index(line, "//"); at >= 0 {
			lines[i] = line[:at]
		}
	}
	return strings.Join(lines, "\n")
}
