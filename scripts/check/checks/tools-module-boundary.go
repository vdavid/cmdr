package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// The AI tool layer's boundary, made self-defending.
//
// `tools/` holds every AI-callable tool, `mcp/` is the transport for outside clients, and
// `agent/` is the in-process runtime. `agent` imports `tools`, and `mcp` imports `tools`.
// What's left of the old `mcp` and `agent` cycle is the short list below: proposal and memory
// handlers write the agent's stores, and the size contract shares the one token estimator.
// Handlers grow the list one convenient import at a time, so it's pinned here, and adding to it
// is a design decision. Why each entry exists: `tools/DETAILS.md` § The two-view registry model.

// toolsAllowedAgentImports are the only `crate::agent` paths `tools/` may name, matched as
// a whole path or a prefix ending at a `::`.
var toolsAllowedAgentImports = []string{
	"store",
	"suggested_ops",
	"memory",
	"chat::budget",
	"types",
	"llm::types",
	"AgentDb",
}

type moduleBoundaryViolation struct {
	relPath string
	line    int
	message string
}

// rustCrateImportPattern finds a path out of the crate root into `agent` or `mcp`, including a
// brace group (`crate::agent::{store, types}`), a group at the root (`crate::{agent, mcp}`),
// and a relative climb (`super::super::agent`). It's a textual scan of comment-free source,
// so a re-export can't hide an import, and a doc comment naming a module can't trip it.
var (
	rustAgentOrMcpPath  = regexp.MustCompile(`\b(?:crate|cmdr_lib|super)::(agent|mcp)\b((?:::(?:[A-Za-z_][A-Za-z0-9_]*))*)(::\{[^}]*\})?`)
	rustCrateRootGroup  = regexp.MustCompile(`\b(?:crate|cmdr_lib)::\{[^}]*\}`)
	rustRootGroupMember = regexp.MustCompile(`\b(agent|mcp)\b`)
)

// RunToolsModuleBoundary pins what `tools/` and `mcp/` may import.
func RunToolsModuleBoundary(ctx *CheckContext) (CheckResult, error) {
	roots, err := ScannerRoots(ctx.RootDir, "tools-module-boundary")
	if err != nil {
		return CheckResult{}, err
	}
	if len(roots) != 1 {
		return CheckResult{}, fmt.Errorf("expected the app's source tree alone, got %v", roots)
	}

	var violations []moduleBoundaryViolation
	scanned := 0
	for _, module := range []string{"tools", "mcp"} {
		moduleDir := filepath.Join(roots[0], module)
		moduleScanned, moduleViolations, scanErr := scanModuleBoundary(ctx.RootDir, moduleDir, module)
		if scanErr != nil {
			return CheckResult{}, fmt.Errorf("failed to scan %s: %w", moduleDir, scanErr)
		}
		// A module that moved away must fail, or this check passes on nothing forever.
		if moduleScanned == 0 {
			return CheckResult{}, fmt.Errorf("found no Rust files in %s; update this check if the module moved", moduleDir)
		}
		scanned += moduleScanned
		violations = append(violations, moduleViolations...)
	}

	if len(violations) > 0 {
		sort.Slice(violations, func(i, j int) bool {
			if violations[i].relPath == violations[j].relPath {
				return violations[i].line < violations[j].line
			}
			return violations[i].relPath < violations[j].relPath
		})
		var sb strings.Builder
		for _, v := range violations {
			sb.WriteString(fmt.Sprintf("%s:%d: %s\n", v.relPath, v.line, v.message))
		}
		return CheckResult{}, fmt.Errorf(
			"found %d %s crossing the tool layer's boundary. `mcp` imports no `agent`, `tools` imports no `mcp`, and `tools` imports only `agent::{%s}`. Move the shared piece into `tools`, or, if the list really has to grow, change `toolsAllowedAgentImports` and `tools/DETAILS.md` together:\n%s",
			len(violations), Pluralize(len(violations), "import", "imports"),
			strings.Join(toolsAllowedAgentImports, ", "), indentOutput(strings.TrimRight(sb.String(), "\n")),
		)
	}

	return Success(fmt.Sprintf(
		"%d Rust %s scanned, the tool layer's boundary holds",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanModuleBoundary(rootDir, moduleDir, module string) (int, []moduleBoundaryViolation, error) {
	var violations []moduleBoundaryViolation
	scanned := 0
	err := corpus.WalkDir(moduleDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}
		scanned++
		source, readErr := corpus.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			relPath = path
		}
		for _, found := range boundaryViolationsIn(module, source) {
			found.relPath = filepath.ToSlash(relPath)
			violations = append(violations, found)
		}
		return nil
	})
	return scanned, violations, err
}

// boundaryViolationsIn scans one file's source. Blanking comments rather than deleting them
// keeps every line number true.
func boundaryViolationsIn(module, source string) []moduleBoundaryViolation {
	code := stripRustLineComments(source)
	var found []moduleBoundaryViolation
	lineOf := func(offset int) int { return strings.Count(code[:offset], "\n") + 1 }

	for _, m := range rustAgentOrMcpPath.FindAllStringSubmatchIndex(code, -1) {
		target := code[m[2]:m[3]]
		text := code[m[0]:m[1]]
		rest := strings.TrimPrefix(code[m[4]:m[5]], "::")
		var group string
		if m[6] >= 0 {
			group = code[m[6]:m[7]]
		}
		if message := judgeBoundaryPath(module, target, rest, group, text); message != "" {
			found = append(found, moduleBoundaryViolation{line: lineOf(m[0]), message: message})
		}
	}
	for _, m := range rustCrateRootGroup.FindAllStringIndex(code, -1) {
		text := code[m[0]:m[1]]
		for _, member := range rustRootGroupMember.FindAllString(text, -1) {
			if (member == "agent") || (member == "mcp" && module == "tools") {
				found = append(found, moduleBoundaryViolation{
					line:    lineOf(m[0]),
					message: fmt.Sprintf("`%s` names `%s` from the crate root, where this check can't see what it reaches", collapseWhitespace(text), member),
				})
			}
		}
	}
	return found
}

// judgeBoundaryPath answers one matched path, or "" when it's allowed. `mcp` naming itself is
// fine, since that's how the transport's own tests reach its files.
func judgeBoundaryPath(module, target, rest, group, text string) string {
	shown := collapseWhitespace(text)
	switch {
	case target == "mcp" && module == "mcp":
		return ""
	case target == "mcp":
		return fmt.Sprintf("`%s`: `tools` doesn't depend on the transport", shown)
	case module == "mcp":
		return fmt.Sprintf("`%s`: `mcp` imports no `agent`", shown)
	}

	if group != "" {
		for _, member := range strings.Split(strings.Trim(strings.TrimPrefix(group, "::"), "{} \n\t"), ",") {
			member = strings.TrimSpace(member)
			if member == "" {
				continue
			}
			path := member
			if rest != "" {
				path = rest + "::" + member
			}
			if !isAllowedAgentImport(path) {
				return fmt.Sprintf("`%s`: `agent::%s` isn't on the list", shown, path)
			}
		}
		return ""
	}
	if !isAllowedAgentImport(rest) {
		return fmt.Sprintf("`%s`: `agent::%s` isn't on the list", shown, rest)
	}
	return ""
}

func isAllowedAgentImport(path string) bool {
	// A `self` member, an alias, or a bare module means the module the rest of the path named.
	path = strings.TrimSuffix(path, "::self")
	if fields := strings.Fields(path); len(fields) > 0 {
		path = fields[0]
	}
	for _, allowed := range toolsAllowedAgentImports {
		if path == allowed || strings.HasPrefix(path, allowed+"::") {
			return true
		}
	}
	return false
}

func collapseWhitespace(s string) string {
	return strings.Join(strings.Fields(s), " ")
}
