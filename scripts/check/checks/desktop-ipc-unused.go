package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// IPC dead-code check: every exported wrapper in `lib/tauri-commands/*.ts` needs
// a production caller outside that folder, and every `commands.*` entry in the
// generated `lib/ipc/bindings.ts` needs a live caller, or an allowlist entry
// with a reason. knip can't see either: it counts the barrel's re-exports as
// uses, `bindings.ts` is generated, and rustc never calls a registered
// `#[tauri::command]` unused. 37 commands and 28 wrappers piled up that way
// before this check existed (`docs/notes/ipc-dead-code-audit.md`).
//
// Mechanics (token scanning, no TS parser):
//   - Production file: a tracked `.ts` / `.svelte` / `.js` under
//     `apps/desktop/src`, minus `*.test.*` / `*.spec.*` and `test-*` harnesses.
//     A `vi.fn()` mock only ever lives in a test file, so it never counts.
//   - Wrapper: an `export function` / `export const` at the top level of a
//     `tauri-commands/*.ts` file other than `index.ts`. It's used when a
//     production file outside the folder names it in an import from a
//     `tauri-commands` path: static, re-export, `await import(...)` with
//     destructuring, or a namespace import's `ns.name`. An import from a
//     sibling wrapper file (`./ipc-types`) counts too: that's folder plumbing.
//   - Command: a top-level key of `export const commands = {…}` in
//     `bindings.ts`, paired with the snake_case name its `__TAURI_INVOKE` call
//     passes. It's used by `commands.<key>` in a production file outside the
//     folder, by `commands.<key>` inside a LIVE top-level declaration of a
//     wrapper file (a used or allowlisted wrapper, or any non-exported helper),
//     or by a raw `invoke('<snake>')` in production code or E2E
//     (`apps/desktop/test/e2e-*`).
//   - Full-line comments (`//`, `*`, `/*`, `<!--`) are dropped first, so a
//     JSDoc mention never keeps a wrapper alive.
//   - Allowlist: `wrappers` keyed by wrapper name, `commands` keyed by the
//     snake_case command name, each with a mandatory reason. An allowlisted
//     wrapper counts as live, so it covers the commands it calls. Entries that
//     no longer earn their place (gone, or now used) shrink-wrap away locally
//     and warn in CI.

const (
	ipcWrappersDir         = "apps/desktop/src/lib/tauri-commands"
	ipcBindingsRelPath     = "apps/desktop/src/lib/ipc/bindings.ts"
	ipcFrontendDir         = "apps/desktop/src"
	ipcE2ERoot             = "apps/desktop/test"
	ipcE2EPrefix           = "apps/desktop/test/e2e-"
	ipcUnusedAllowlistName = "desktop-ipc-unused-allowlist.json"
)

var (
	// A top-level declaration in a wrapper file starts at column 0.
	ipcTopLevelDeclPattern  = regexp.MustCompile(`^(?:export\s+)?(?:declare\s+)?(?:async\s+function|function|const|let|var|class|type|interface|enum|import|export)\b`)
	ipcExportedFuncPattern  = regexp.MustCompile(`^export\s+(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)`)
	ipcExportedConstPattern = regexp.MustCompile(`^export\s+(?:const|let)\s+([A-Za-z_$][\w$]*)`)

	// `import { a, type B, c as d } from '…'` and `export { a } from '…'`.
	ipcStaticImportPattern = regexp.MustCompile(`(?s)\b(?:import|export)\s+(?:type\s+)?\{([^{}]*)\}\s*from\s*['"]([^'"]+)['"]`)
	// `const { a, b } = await import('…')`.
	ipcDynamicImportPattern = regexp.MustCompile(`(?s)\{([^{}]*)\}\s*=\s*await\s+import\(\s*['"]([^'"]+)['"]\s*\)`)
	// `(await import('…')).name`.
	ipcDynamicMemberPattern = regexp.MustCompile(`import\(\s*['"]([^'"]+)['"]\s*\)\s*\)\s*\.\s*([A-Za-z_$][\w$]*)`)
	// `import * as ns from '…'`.
	ipcNamespaceImportPattern = regexp.MustCompile(`import\s+\*\s+as\s+([A-Za-z_$][\w$]*)\s+from\s*['"]([^'"]+)['"]`)
	// A module path that lands in the wrapper folder: the barrel or a sub-file.
	ipcWrapperModulePattern = regexp.MustCompile(`(?:^|/)tauri-commands(?:/[\w.-]+)?$`)

	ipcCommandsRefPattern = regexp.MustCompile(`\bcommands\s*\.\s*([A-Za-z_$][\w$]*)`)
	// `invoke('x')`, `invoke<T>('x')`, `__TAURI_INTERNALS__.invoke('x')`.
	ipcRawInvokePattern = regexp.MustCompile("\\binvoke\\s*(?:<[^()]*>)?\\(\\s*['\"`]([a-z][a-z0-9_]*)['\"`]")

	ipcBindingsKeyPattern    = regexp.MustCompile(`^  ([A-Za-z_$][\w$]*)\s*:`)
	ipcBindingsInvokePattern = regexp.MustCompile(`(?s)__TAURI_INVOKE(?:<.*?>)?\(\s*'([^']+)'`)
)

type ipcUnusedAllowlist struct {
	Comment  string            `json:"$comment,omitempty"`
	Wrappers map[string]string `json:"wrappers"`
	Commands map[string]string `json:"commands"`
}

func ipcUnusedAllowlistPath(rootDir string) string {
	return filepath.Join(rootDir, "scripts", "check", "checks", ipcUnusedAllowlistName)
}

func loadIpcUnusedAllowlist(rootDir string) (ipcUnusedAllowlist, error) {
	list := ipcUnusedAllowlist{}
	data, err := os.ReadFile(ipcUnusedAllowlistPath(rootDir))
	if err != nil && !os.IsNotExist(err) {
		return list, err
	}
	if err == nil {
		if err := json.Unmarshal(data, &list); err != nil {
			return list, fmt.Errorf("parse %s: %w", ipcUnusedAllowlistName, err)
		}
	}
	if list.Wrappers == nil {
		list.Wrappers = map[string]string{}
	}
	if list.Commands == nil {
		list.Commands = map[string]string{}
	}
	return list, nil
}

// ipcSegment is one top-level declaration of a wrapper file, with the text up
// to the next one.
type ipcSegment struct {
	name     string // declared name; "" for imports, types, and the like
	exported bool   // an `export function` / `export const`
	text     string
}

type ipcWrapper struct {
	name string
	file string
}

type ipcCommand struct {
	key   string // camelCase key in `commands`
	snake string // the name `__TAURI_INVOKE` passes
}

type ipcScan struct {
	wrappers    []ipcWrapper
	wrapperUsed map[string]bool
	segments    map[string][]ipcSegment // wrapper file -> segments
	commands    []ipcCommand
	outsideCmds map[string]bool // `commands.<key>` outside the wrapper folder
	rawInvokes  map[string]bool // snake names invoked raw, outside the folder or in E2E
}

func isIpcScannedSource(rel string) bool {
	switch path.Ext(rel) {
	case ".ts", ".svelte", ".js":
		return true
	}
	return false
}

func isIpcTestFile(rel string) bool {
	base := path.Base(rel)
	return strings.Contains(base, ".test.") || strings.Contains(base, ".spec.") || strings.HasPrefix(base, "test-")
}

// stripIpcCommentLines drops full-line comments so a doc mention never counts
// as a use. Trailing comments stay: stripping them safely needs a tokenizer
// (`'https://…'`, `'**/*.ts'`), and they rarely name a wrapper.
func stripIpcCommentLines(src string) string {
	lines := strings.Split(src, "\n")
	out := lines[:0]
	for _, line := range lines {
		t := strings.TrimSpace(line)
		if strings.HasPrefix(t, "//") || strings.HasPrefix(t, "*") || strings.HasPrefix(t, "/*") || strings.HasPrefix(t, "<!--") {
			continue
		}
		out = append(out, line)
	}
	return strings.Join(out, "\n")
}

// importClauseNames returns the imported binding names in `a, type B, c as d`
// (the source-side names: `a`, `B`, `c`).
func importClauseNames(clause string) []string {
	var names []string
	for _, part := range strings.Split(clause, ",") {
		part = strings.TrimSpace(part)
		part = strings.TrimPrefix(part, "type ")
		part = strings.TrimSpace(part)
		if i := strings.IndexAny(part, " \t\n:"); i >= 0 {
			part = part[:i]
		}
		if part != "" {
			names = append(names, part)
		}
	}
	return names
}

// collectWrapperImports adds every name `src` imports from a tauri-commands path.
func collectWrapperImports(src string, used map[string]bool) {
	for _, m := range ipcStaticImportPattern.FindAllStringSubmatch(src, -1) {
		if ipcWrapperModulePattern.MatchString(m[2]) {
			for _, n := range importClauseNames(m[1]) {
				used[n] = true
			}
		}
	}
	for _, m := range ipcDynamicImportPattern.FindAllStringSubmatch(src, -1) {
		if ipcWrapperModulePattern.MatchString(m[2]) {
			for _, n := range importClauseNames(m[1]) {
				used[n] = true
			}
		}
	}
	for _, m := range ipcDynamicMemberPattern.FindAllStringSubmatch(src, -1) {
		if ipcWrapperModulePattern.MatchString(m[1]) {
			used[m[2]] = true
		}
	}
	for _, m := range ipcNamespaceImportPattern.FindAllStringSubmatch(src, -1) {
		if !ipcWrapperModulePattern.MatchString(m[2]) {
			continue
		}
		member := regexp.MustCompile(`\b` + regexp.QuoteMeta(m[1]) + `\s*\.\s*([A-Za-z_$][\w$]*)`)
		for _, mm := range member.FindAllStringSubmatch(src, -1) {
			used[mm[1]] = true
		}
	}
}

// collectSiblingImports adds every name a wrapper file imports from a sibling
// (`./ipc-types`): folder plumbing like `throwIpcError`, which no caller outside
// needs. The barrel's `index.ts` is never scanned, so its re-exports don't count.
func collectSiblingImports(src string, used map[string]bool) {
	for _, m := range ipcStaticImportPattern.FindAllStringSubmatch(src, -1) {
		if strings.HasPrefix(m[2], "./") && !strings.Contains(m[2][2:], "/") {
			for _, n := range importClauseNames(m[1]) {
				used[n] = true
			}
		}
	}
}

func collectRawInvokes(src string, into map[string]bool) {
	for _, m := range ipcRawInvokePattern.FindAllStringSubmatch(src, -1) {
		into[m[1]] = true
	}
}

// splitIpcSegments cuts a wrapper file into its top-level declarations.
func splitIpcSegments(src string) []ipcSegment {
	var segs []ipcSegment
	cur := ipcSegment{}
	var body strings.Builder
	flush := func() {
		cur.text = body.String()
		segs = append(segs, cur)
		body.Reset()
	}
	for _, line := range strings.Split(src, "\n") {
		if ipcTopLevelDeclPattern.MatchString(line) {
			flush()
			cur = ipcSegment{}
			if m := ipcExportedFuncPattern.FindStringSubmatch(line); m != nil {
				cur = ipcSegment{name: m[1], exported: true}
			} else if m := ipcExportedConstPattern.FindStringSubmatch(line); m != nil {
				cur = ipcSegment{name: m[1], exported: true}
			}
		}
		body.WriteString(line)
		body.WriteString("\n")
	}
	flush()
	return segs
}

// parseIpcBindingsCommands reads the `commands` object's top-level keys and the
// snake_case name each one invokes.
func parseIpcBindingsCommands(src string) []ipcCommand {
	lines := strings.Split(src, "\n")
	start := -1
	for i, line := range lines {
		if strings.HasPrefix(line, "export const commands = {") {
			start = i + 1
			break
		}
	}
	if start < 0 {
		return nil
	}
	var cmds []ipcCommand
	key := ""
	var body strings.Builder
	flush := func() {
		if key == "" {
			return
		}
		if m := ipcBindingsInvokePattern.FindStringSubmatch(body.String()); m != nil {
			cmds = append(cmds, ipcCommand{key: key, snake: m[1]})
		}
	}
	for _, line := range lines[start:] {
		if line == "}" {
			break
		}
		if m := ipcBindingsKeyPattern.FindStringSubmatch(line); m != nil {
			flush()
			key = m[1]
			body.Reset()
		}
		body.WriteString(line)
		body.WriteString("\n")
	}
	flush()
	return cmds
}

func scanIpcUsage(rootDir string) (ipcScan, error) {
	s := ipcScan{
		wrapperUsed: map[string]bool{},
		segments:    map[string][]ipcSegment{},
		outsideCmds: map[string]bool{},
		rawInvokes:  map[string]bool{},
	}
	frontend, err := readIpcSources(rootDir, ipcFrontendDir, func(rel string) bool {
		return !isIpcTestFile(rel) && rel != ipcBindingsRelPath
	})
	if err != nil {
		return s, err
	}
	for _, f := range frontend {
		s.addFrontendFile(f.rel, f.src)
	}

	e2e, err := readIpcSources(rootDir, ipcE2ERoot, func(rel string) bool {
		return strings.HasPrefix(rel, ipcE2EPrefix)
	})
	if err != nil {
		return s, err
	}
	for _, f := range e2e {
		collectRawInvokes(f.src, s.rawInvokes)
	}

	bindings, present, err := readTrackedFile(rootDir, ipcBindingsRelPath)
	if err != nil {
		return s, err
	}
	if present {
		s.commands = parseIpcBindingsCommands(string(bindings))
	}
	sort.Slice(s.wrappers, func(i, j int) bool { return s.wrappers[i].name < s.wrappers[j].name })
	return s, nil
}

type ipcSource struct {
	rel string
	src string // with full-line comments stripped
}

// readIpcSources reads the tracked scanned sources under dir that keep passes.
func readIpcSources(rootDir, dir string, keep func(rel string) bool) ([]ipcSource, error) {
	tracked, err := listTrackedFiles(rootDir, dir)
	if err != nil {
		return nil, err
	}
	var out []ipcSource
	for _, rel := range tracked {
		if !isIpcScannedSource(rel) || !keep(rel) {
			continue
		}
		data, present, err := readTrackedFile(rootDir, rel)
		if err != nil {
			return nil, err
		}
		if present {
			out = append(out, ipcSource{rel: rel, src: stripIpcCommentLines(string(data))})
		}
	}
	return out, nil
}

// addFrontendFile records a production file: a wrapper file's declarations, or
// a caller's imports, `commands.*` references, and raw invokes.
func (s *ipcScan) addFrontendFile(rel, src string) {
	if path.Dir(rel) == ipcWrappersDir {
		if path.Base(rel) == "index.ts" {
			return
		}
		collectSiblingImports(src, s.wrapperUsed)
		segs := splitIpcSegments(src)
		s.segments[rel] = segs
		for _, seg := range segs {
			if seg.exported {
				s.wrappers = append(s.wrappers, ipcWrapper{name: seg.name, file: rel})
			}
		}
		return
	}
	if strings.HasPrefix(rel, ipcWrappersDir+"/") {
		return // a nested folder under tauri-commands: not a wrapper file, not a caller
	}
	collectWrapperImports(src, s.wrapperUsed)
	for _, m := range ipcCommandsRefPattern.FindAllStringSubmatch(src, -1) {
		s.outsideCmds[m[1]] = true
	}
	collectRawInvokes(src, s.rawInvokes)
}

type ipcUnusedResult struct {
	deadWrappers   []ipcWrapper
	deadCommands   []ipcCommand
	missingReasons []string
	staleChanges   []string
	allowlisted    int
}

// evaluateIpcUsage decides which wrappers and commands are dead, and prunes
// stale allowlist entries from `list` in place.
func evaluateIpcUsage(s ipcScan, list *ipcUnusedAllowlist) ipcUnusedResult {
	var r ipcUnusedResult

	wrapperExists := map[string]bool{}
	wrapperLive := map[string]bool{}
	for _, w := range s.wrappers {
		wrapperExists[w.name] = true
		_, allowed := list.Wrappers[w.name]
		switch {
		case s.wrapperUsed[w.name]:
			wrapperLive[w.name] = true
		case allowed:
			wrapperLive[w.name] = true
			r.allowlisted++
		default:
			r.deadWrappers = append(r.deadWrappers, w)
		}
	}

	liveCmds, liveRaw := liveWrapperCalls(s, wrapperLive)
	commandExists := map[string]bool{}
	commandUsed := map[string]bool{}
	for _, c := range s.commands {
		commandExists[c.snake] = true
		if s.outsideCmds[c.key] || liveCmds[c.key] || s.rawInvokes[c.snake] || liveRaw[c.snake] {
			commandUsed[c.snake] = true
			continue
		}
		if _, allowed := list.Commands[c.snake]; allowed {
			r.allowlisted++
			continue
		}
		r.deadCommands = append(r.deadCommands, c)
	}

	pruneIpcAllowlistSection(list.Wrappers, "wrapper", "no longer exported from tauri-commands/", wrapperExists, s.wrapperUsed, &r)
	pruneIpcAllowlistSection(list.Commands, "command", "no longer in bindings.ts", commandExists, commandUsed, &r)
	return r
}

// liveWrapperCalls collects what the LIVE top-level declarations of the wrapper
// files call: `commands.<key>` references and raw invoke names.
func liveWrapperCalls(s ipcScan, wrapperLive map[string]bool) (cmds, raw map[string]bool) {
	cmds = map[string]bool{}
	raw = map[string]bool{}
	for _, segs := range s.segments {
		for _, seg := range segs {
			if seg.exported && !wrapperLive[seg.name] {
				continue
			}
			for _, m := range ipcCommandsRefPattern.FindAllStringSubmatch(seg.text, -1) {
				cmds[m[1]] = true
			}
			collectRawInvokes(seg.text, raw)
		}
	}
	return cmds, raw
}

// pruneIpcAllowlistSection drops entries whose subject is gone or now used, and
// records the ones without a reason.
func pruneIpcAllowlistSection(section map[string]string, kind, goneWhy string, exists, used map[string]bool, r *ipcUnusedResult) {
	for _, name := range sortedKeys(section) {
		switch {
		case !exists[name]:
			delete(section, name)
			r.staleChanges = append(r.staleChanges, fmt.Sprintf("removed %s %s (%s)", kind, name, goneWhy))
		case used[name]:
			delete(section, name)
			r.staleChanges = append(r.staleChanges, fmt.Sprintf("removed %s %s (now has a caller)", kind, name))
		case strings.TrimSpace(section[name]) == "":
			r.missingReasons = append(r.missingReasons, kind+"s."+name)
		}
	}
}

// RunIpcUnused fails on an IPC wrapper or command that nothing calls and no
// allowlist entry explains.
func RunIpcUnused(ctx *CheckContext) (CheckResult, error) {
	list, err := loadIpcUnusedAllowlist(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	scan, err := scanIpcUsage(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	if len(scan.commands) == 0 {
		return CheckResult{}, fmt.Errorf("found no `commands` entries in %s; has the tauri-specta output shape changed?", ipcBindingsRelPath)
	}
	r := evaluateIpcUsage(scan, &list)

	if len(r.staleChanges) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(ipcUnusedAllowlistPath(ctx.RootDir), list); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, "scripts/check/checks/"+ipcUnusedAllowlistName)
	}
	staleMsg := formatDocsStaleMsg(ctx.CI, r.staleChanges)

	if len(r.deadWrappers) == 0 && len(r.deadCommands) == 0 && len(r.missingReasons) == 0 {
		okMsg := fmt.Sprintf("%d %s and %d %s all have a caller",
			len(scan.wrappers), Pluralize(len(scan.wrappers), "wrapper", "wrappers"),
			len(scan.commands), Pluralize(len(scan.commands), "command", "commands"))
		if r.allowlisted > 0 {
			okMsg = fmt.Sprintf("%s (%d allowlisted)", okMsg, r.allowlisted)
		}
		if staleMsg == "" {
			return Success(okMsg), nil
		}
		if ctx.CI {
			return CheckResult{Code: ResultWarning, Message: okMsg + "; " + staleMsg, Total: -1, Issues: -1, Changes: -1}, nil
		}
		return SuccessWithChanges(okMsg + "; " + staleMsg), nil
	}

	body := formatIpcUnusedFailure(r)
	if staleMsg != "" {
		body += "\n" + staleMsg
	}
	return CheckResult{}, fmt.Errorf("%s", body)
}

func formatIpcUnusedFailure(r ipcUnusedResult) string {
	var sb strings.Builder
	if n := len(r.deadWrappers); n > 0 {
		sb.WriteString(fmt.Sprintf("%d IPC %s in tauri-commands/ %s no production caller outside the folder:\n",
			n, Pluralize(n, "wrapper", "wrappers"), Pluralize(n, "has", "have")))
		for _, w := range r.deadWrappers {
			sb.WriteString(fmt.Sprintf("  - %s (%s)\n", w.name, w.file))
		}
	}
	if n := len(r.deadCommands); n > 0 {
		sb.WriteString(fmt.Sprintf("%d %s in bindings.ts %s no live caller (only dead wrappers, tests, or nothing):\n",
			n, Pluralize(n, "command", "commands"), Pluralize(n, "has", "have")))
		for _, c := range r.deadCommands {
			sb.WriteString(fmt.Sprintf("  - commands.%s (%s)\n", c.key, c.snake))
		}
	}
	if n := len(r.missingReasons); n > 0 {
		sb.WriteString(fmt.Sprintf("%d allowlist %s without a reason:\n", n, Pluralize(n, "entry", "entries")))
		for _, k := range r.missingReasons {
			sb.WriteString(fmt.Sprintf("  - %s\n", k))
		}
	}
	sb.WriteString("Delete what's dead (the wrapper, its barrel line, the `#[tauri::command]`, and its registration, then `pnpm bindings:regen`), " +
		"wire up what's missing its UI, or allowlist it with a reason in scripts/check/checks/" + ipcUnusedAllowlistName + ". " +
		"See apps/desktop/src/lib/tauri-commands/DETAILS.md § \"Unused wrappers and commands\".")
	return sb.String()
}
