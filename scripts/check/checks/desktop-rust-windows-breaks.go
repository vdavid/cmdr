package checks

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"sort"
	"strings"
)

// The Windows break report: for every workspace member that builds on Linux but
// isn't yet declared for Windows (`platforms` in its manifest, the ratchet
// `workspace-member-coverage` holds), one `cargo check --keep-going` says which
// compile, which fail in their own code (with their errors), and which stop at a
// dependency that fails. It never fails on a break: the members that must compile
// are `desktop-rust-clippy`'s on a Windows host, and this is the map of the rest.
// `docs/tooling/ci.md` § Windows lane.

// windowsBreaksTargetEnv names a target triple to cross-check from a host that
// isn't Windows (`x86_64-pc-windows-gnu` with mingw-w64 installed is the cheap
// one). On a Windows host the check runs natively and ignores it.
const windowsBreaksTargetEnv = "CMDR_WINDOWS_BREAKS_TARGET"

// windowsBreaksDirEnv names the directory the report goes to. The Windows lane
// points it at the runner's temp dir and uploads it; locally it defaults to
// `target/windows-breaks/` under the repo root.
const windowsBreaksDirEnv = "CMDR_WINDOWS_BREAKS_DIR"

// windowsBreaksFirstErrors is how many of a member's errors the summary quotes;
// `errors.md` beside it holds every one.
const windowsBreaksFirstErrors = 3

// RunWindowsBreaks writes the Windows break report.
func RunWindowsBreaks(ctx *CheckContext) (CheckResult, error) {
	target, ok := windowsBreaksTarget(runtime.GOOS, os.Getenv(windowsBreaksTargetEnv))
	if !ok {
		return Skipped(fmt.Sprintf("needs a Windows host, or %s=<triple> to cross-check", windowsBreaksTargetEnv)), nil
	}

	members, err := WorkspaceMembers(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	attempted := windowsPendingMembers(members)
	if len(attempted) == 0 {
		return Success("Every member that builds on Linux is declared for Windows; nothing to report"), nil
	}

	reportDir := os.Getenv(windowsBreaksDirEnv)
	if reportDir == "" {
		reportDir = filepath.Join(ctx.RootDir, "target", "windows-breaks")
	}
	if err := os.MkdirAll(reportDir, 0o755); err != nil {
		return CheckResult{}, fmt.Errorf("couldn't create the report directory: %w", err)
	}

	report, err := buildWindowsBreakReport(ctx.RootDir, target, attempted)
	if err != nil {
		return CheckResult{}, err
	}
	summary := report.summaryMarkdown()
	files := map[string]string{"summary.md": summary, "errors.md": report.errorsMarkdown()}
	for name, content := range files {
		if err := os.WriteFile(filepath.Join(reportDir, name), []byte(content), 0o644); err != nil {
			return CheckResult{}, fmt.Errorf("couldn't write %s: %w", name, err)
		}
	}

	result := Success(report.oneLine() + " (report: " + reportDir + ")")
	result.Total = len(attempted)
	result.Details = summary
	return result, nil
}

// windowsBreaksTarget says which triple to check, "" meaning the host's own.
func windowsBreaksTarget(goos, envTarget string) (string, bool) {
	if goos == "windows" {
		return "", true
	}
	if envTarget != "" {
		return envTarget, true
	}
	return "", false
}

// windowsPendingMembers are the members the report covers: built on Linux, not
// declared for Windows. A macOS-only member (`cmdr-fsevent-stream`) is Apple by
// design, not a port in progress, so it stays out.
func windowsPendingMembers(members []WorkspaceMember) []WorkspaceMember {
	var out []WorkspaceMember
	for _, m := range members {
		if m.BuildsOn("linux") && !m.BuildsOn("windows") {
			out = append(out, m)
		}
	}
	return out
}

// windowsMemberStatus is what the report says about one member.
type windowsMemberStatus string

const (
	windowsCompiles windowsMemberStatus = "compiles"
	windowsFails    windowsMemberStatus = "fails"
	windowsBlocked  windowsMemberStatus = "blocked"
	windowsNotBuilt windowsMemberStatus = "not built"
)

// windowsCompileError is one distinct error cargo reported for a package.
type windowsCompileError struct {
	Message  string // the headline, `mismatched types`
	Location string // the primary span, `crates/cmdr-fs/src/lib.rs:12`, or ""
	Rendered string // rustc's full rendering, for errors.md
}

// windowsMemberResult is one attempted member's line in the report.
type windowsMemberResult struct {
	Name     string
	Status   windowsMemberStatus
	Errors   []windowsCompileError
	Blockers []string // failing packages in its dependency closure, sorted
}

// windowsBreakReport is the whole report.
type windowsBreakReport struct {
	Target  string // the triple, or "" for native
	Members []windowsMemberResult
	// FailedDeps are third-party packages that failed (their own errors or a build
	// script), each with its first error line.
	FailedDeps map[string]string
	// CargoStopped holds cargo's own error lines when it stopped before compiling
	// anything (dependency resolution, a missing toolchain): then every member
	// reads "not built", and these lines say why.
	CargoStopped []string
}

// buildWindowsBreakReport runs `cargo metadata` for the dependency graph and one
// `cargo check --keep-going` over the attempted members, and classifies each.
func buildWindowsBreakReport(rootDir, target string, attempted []WorkspaceMember) (windowsBreakReport, error) {
	report := windowsBreakReport{Target: target, FailedDeps: map[string]string{}}
	names := make([]string, 0, len(attempted))
	for _, m := range attempted {
		names = append(names, m.Name)
	}

	graph, stopped, err := readWindowsDepGraph(rootDir, target)
	if err != nil {
		return report, err
	}
	if stopped != nil {
		report.CargoStopped = stopped
		for _, name := range names {
			report.Members = append(report.Members, windowsMemberResult{Name: name, Status: windowsNotBuilt})
		}
		return report, nil
	}

	stdout, stderr, err := runWindowsCheck(rootDir, target, names)
	if err != nil {
		return report, err
	}
	report.assemble(names, graph, parseCargoCheckOutput(stdout, stderr, graph), stderr)
	return report, nil
}

// readWindowsDepGraph reads the dependency graph for the checked triple only:
// unfiltered, it holds every OS's edges, and a `cfg(unix)` dependency would read
// as blocking the members above it. When cargo can't resolve the graph at all,
// its error lines come back as `stopped`, which is itself the finding.
func readWindowsDepGraph(rootDir, target string) (graph windowsDepGraph, stopped []string, err error) {
	triple := target
	if triple == "" {
		if triple, err = rustHostTriple(rootDir); err != nil {
			return graph, nil, err
		}
	}
	cmd := exec.Command("cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", triple)
	cmd.Dir = rootDir
	var stdout, stderr bytes.Buffer
	if runErr := RunCommandTo(cmd, &stdout, &stderr); runErr != nil {
		return graph, append([]string{"cargo metadata: " + runErr.Error()}, cargoErrorLines(stderr.String())...), nil
	}
	graph, err = parseWindowsDepGraph(stdout.Bytes())
	return graph, nil, err
}

// runWindowsCheck is the one `cargo check` over every attempted member.
func runWindowsCheck(rootDir, target string, names []string) (stdout []byte, stderr string, err error) {
	selected := map[string]bool{}
	args := []string{"check", "--locked", "--keep-going", "--all-targets", "--message-format=json"}
	if target != "" {
		args = append(args, "--target", target)
	}
	for _, name := range names {
		args = append(args, "-p", name)
		selected[name] = true
	}
	args = append(args, SharedTargetFeatureArgsFor(selected)...)
	cmd := exec.Command("cargo", args...)
	cmd.Dir = rootDir
	var out, errOut bytes.Buffer
	// A non-zero exit is the expected case (something fails); what failed is in
	// the messages, and a cargo that couldn't start at all says so on stderr.
	if runErr := RunCommandTo(cmd, &out, &errOut); runErr != nil && out.Len() == 0 && errOut.Len() == 0 {
		return nil, "", fmt.Errorf("couldn't run cargo: %w", runErr)
	}
	return out.Bytes(), errOut.String(), nil
}

// assemble fills the report from one check's outcome.
func (r *windowsBreakReport) assemble(names []string, graph windowsDepGraph, outcome cargoCheckOutcome, stderr string) {
	for id, line := range outcome.firstErrorByPackage() {
		if !graph.isWorkspace(id) {
			r.FailedDeps[graph.label(id)] = line
		}
	}
	if len(outcome.built) == 0 && len(outcome.failed) == 0 {
		r.CargoStopped = cargoErrorLines(stderr)
	}
	for _, name := range names {
		r.Members = append(r.Members, classifyWindowsMember(name, graph, outcome))
	}
}

// classifyWindowsMember decides one member's status from the check's outcome.
func classifyWindowsMember(name string, graph windowsDepGraph, outcome cargoCheckOutcome) windowsMemberResult {
	result := windowsMemberResult{Name: name}
	id, ok := graph.workspaceID(name)
	if !ok {
		result.Status = windowsNotBuilt
		return result
	}
	if outcome.failed[id] {
		result.Status = windowsFails
		result.Errors = outcome.errors[id]
		return result
	}
	for dep := range graph.closure(id) {
		if dep != id && outcome.failed[dep] {
			result.Blockers = append(result.Blockers, graph.label(dep))
		}
	}
	sort.Strings(result.Blockers)
	switch {
	case len(result.Blockers) > 0:
		result.Status = windowsBlocked
	case outcome.built[id]:
		result.Status = windowsCompiles
	default:
		result.Status = windowsNotBuilt
	}
	return result
}

// windowsDepGraph is the slice of `cargo metadata` the report needs.
type windowsDepGraph struct {
	names     map[string]string // package id -> name
	versions  map[string]string // package id -> version
	workspace map[string]string // workspace member name -> package id
	deps      map[string][]windowsDep
}

type windowsDep struct {
	id  string
	dev bool // only a dev-dependency edge (tests, benches, examples)
}

type windowsMetadataJSON struct {
	Packages []struct {
		ID      string `json:"id"`
		Name    string `json:"name"`
		Version string `json:"version"`
	} `json:"packages"`
	WorkspaceMembers []string `json:"workspace_members"`
	Resolve          struct {
		Nodes []struct {
			ID   string `json:"id"`
			Deps []struct {
				Pkg      string `json:"pkg"`
				DepKinds []struct {
					Kind *string `json:"kind"`
				} `json:"dep_kinds"`
			} `json:"deps"`
		} `json:"nodes"`
	} `json:"resolve"`
}

func parseWindowsDepGraph(data []byte) (windowsDepGraph, error) {
	var meta windowsMetadataJSON
	if err := json.Unmarshal(data, &meta); err != nil {
		return windowsDepGraph{}, fmt.Errorf("couldn't parse `cargo metadata`: %w", err)
	}
	g := windowsDepGraph{
		names: map[string]string{}, versions: map[string]string{},
		workspace: map[string]string{}, deps: map[string][]windowsDep{},
	}
	for _, p := range meta.Packages {
		g.names[p.ID] = p.Name
		g.versions[p.ID] = p.Version
	}
	for _, id := range meta.WorkspaceMembers {
		g.workspace[g.names[id]] = id
	}
	for _, node := range meta.Resolve.Nodes {
		for _, d := range node.Deps {
			dev := len(d.DepKinds) > 0
			for _, k := range d.DepKinds {
				if k.Kind == nil || *k.Kind != "dev" {
					dev = false
				}
			}
			g.deps[node.ID] = append(g.deps[node.ID], windowsDep{id: d.Pkg, dev: dev})
		}
	}
	return g, nil
}

func (g windowsDepGraph) workspaceID(name string) (string, bool) {
	id, ok := g.workspace[name]
	return id, ok
}

func (g windowsDepGraph) isWorkspace(id string) bool {
	_, ok := g.workspace[g.names[id]]
	return ok && g.workspace[g.names[id]] == id
}

// label names a package for the report: a member by name, a third-party crate
// with its version.
func (g windowsDepGraph) label(id string) string {
	if g.isWorkspace(id) {
		return g.names[id]
	}
	return g.names[id] + " " + g.versions[id]
}

// closure is every package id reaches: all its own edges (an `--all-targets`
// check builds its tests, so its dev-dependencies too), then only normal and
// build edges below it, since a dependency's dev-dependencies are never built.
func (g windowsDepGraph) closure(id string) map[string]bool {
	seen := map[string]bool{id: true}
	queue := []string{id}
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		for _, d := range g.deps[cur] {
			if seen[d.id] || (d.dev && cur != id) {
				continue
			}
			seen[d.id] = true
			queue = append(queue, d.id)
		}
	}
	return seen
}

// cargoCheckOutcome is what one `cargo check --message-format=json` reported.
type cargoCheckOutcome struct {
	errors map[string][]windowsCompileError // package id -> its distinct errors, in order
	failed map[string]bool                  // package id -> it failed (errors or a build script)
	built  map[string]bool                  // package id -> at least one of its units compiled
}

func (o cargoCheckOutcome) firstErrorByPackage() map[string]string {
	out := map[string]string{}
	for id, errs := range o.errors {
		out[id] = errs[0].Message
	}
	return out
}

type cargoCheckMessage struct {
	Reason    string `json:"reason"`
	PackageID string `json:"package_id"`
	Message   *struct {
		Level    string `json:"level"`
		Message  string `json:"message"`
		Rendered string `json:"rendered"`
		Spans    []struct {
			FileName  string `json:"file_name"`
			LineStart int    `json:"line_start"`
			IsPrimary bool   `json:"is_primary"`
		} `json:"spans"`
	} `json:"message"`
}

// buildScriptFailureRe matches cargo's stderr line for a failed build script,
// the one failure that reaches no JSON message: "failed to run custom build
// command for `aws-lc-sys v0.30.0`".
var buildScriptFailureRe = regexp.MustCompile("failed to run custom build command for `([^` ]+) v([^`]+)`")

func parseCargoCheckOutput(stdout []byte, stderr string, graph windowsDepGraph) cargoCheckOutcome {
	o := cargoCheckOutcome{
		errors: map[string][]windowsCompileError{}, failed: map[string]bool{}, built: map[string]bool{},
	}
	seen := map[string]bool{}
	scanner := bufio.NewScanner(bytes.NewReader(stdout))
	scanner.Buffer(make([]byte, 0, 64*1024), 16*1024*1024)
	for scanner.Scan() {
		var msg cargoCheckMessage
		if json.Unmarshal(scanner.Bytes(), &msg) == nil {
			o.add(msg, seen)
		}
	}
	o.addBuildScriptFailures(stderr, graph)
	return o
}

// add records one JSON message: a compiled unit, or a distinct error.
func (o cargoCheckOutcome) add(msg cargoCheckMessage, seen map[string]bool) {
	switch msg.Reason {
	case "compiler-artifact":
		o.built[msg.PackageID] = true
	case "compiler-message":
		if msg.Message == nil || msg.Message.Level != "error" || strings.HasPrefix(msg.Message.Message, "aborting due to") {
			return
		}
		o.failed[msg.PackageID] = true
		// `--all-targets` reports a lib's error twice, once for the lib and once
		// for its test build; one is enough.
		key := msg.PackageID + "\x00" + msg.Message.Rendered
		if seen[key] {
			return
		}
		seen[key] = true
		e := windowsCompileError{Message: msg.Message.Message, Rendered: msg.Message.Rendered}
		for _, span := range msg.Message.Spans {
			if span.IsPrimary {
				e.Location = fmt.Sprintf("%s:%d", filepath.ToSlash(span.FileName), span.LineStart)
				break
			}
		}
		o.errors[msg.PackageID] = append(o.errors[msg.PackageID], e)
	}
}

// addBuildScriptFailures records each failed build script from cargo's stderr.
func (o cargoCheckOutcome) addBuildScriptFailures(stderr string, graph windowsDepGraph) {
	for _, line := range strings.Split(stderr, "\n") {
		m := buildScriptFailureRe.FindStringSubmatch(line)
		if m == nil {
			continue
		}
		trimmed := strings.TrimSpace(line)
		for id, name := range graph.names {
			if name == m[1] && graph.versions[id] == m[2] {
				o.failed[id] = true
				o.errors[id] = append(o.errors[id], windowsCompileError{
					Message:  strings.TrimSpace(strings.TrimPrefix(trimmed, "error:")),
					Rendered: trimmed,
				})
			}
		}
	}
}

// cargoErrorLines keeps cargo's own `error…` lines and what follows each up to
// the next blank line, which is where cargo explains a resolution or toolchain
// failure.
func cargoErrorLines(stderr string) []string {
	var out []string
	inError := false
	for _, line := range strings.Split(stderr, "\n") {
		trimmed := strings.TrimSpace(line)
		switch {
		case strings.HasPrefix(trimmed, "error"):
			inError = true
		case trimmed == "":
			inError = false
		}
		if inError {
			out = append(out, strings.TrimRight(line, "\r"))
		}
	}
	return out
}

func (r windowsBreakReport) counts() map[windowsMemberStatus]int {
	c := map[windowsMemberStatus]int{}
	for _, m := range r.Members {
		c[m.Status]++
	}
	return c
}

func (r windowsBreakReport) totalErrors() int {
	n := 0
	for _, m := range r.Members {
		n += len(m.Errors)
	}
	return n
}

func (r windowsBreakReport) where() string {
	if r.Target == "" {
		return "native Windows"
	}
	return "`" + r.Target + "`"
}

// oneLine is the check's result line.
func (r windowsBreakReport) oneLine() string {
	c := r.counts()
	n := len(r.Members)
	line := fmt.Sprintf("%d %s not yet declared for Windows, checked on %s: %d %s, %d %s with %d %s, %d blocked",
		n, Pluralize(n, "member", "members"), strings.Trim(r.where(), "`"),
		c[windowsCompiles], Pluralize(c[windowsCompiles], "compiles", "compile"),
		c[windowsFails], Pluralize(c[windowsFails], "fails", "fail"),
		r.totalErrors(), Pluralize(r.totalErrors(), "error", "errors"),
		c[windowsBlocked])
	if c[windowsNotBuilt] > 0 {
		line += fmt.Sprintf(", %d not built", c[windowsNotBuilt])
	}
	return line
}

// summaryMarkdown is the job summary: one table row per member, the members ready
// to join the must-compile list, failing dependencies, and each failing member's
// first errors.
func (r windowsBreakReport) summaryMarkdown() string {
	var sb strings.Builder
	sb.WriteString("## Windows break report\n\n")
	sb.WriteString(r.oneLine() + ".\n\n")
	if len(r.CargoStopped) > 0 {
		sb.WriteString("Cargo stopped before compiling anything:\n\n```\n")
		sb.WriteString(strings.Join(r.CargoStopped, "\n"))
		sb.WriteString("\n```\n\n")
	}

	sb.WriteString("| Member | Status | Errors | First error, or what blocks it |\n")
	sb.WriteString("| --- | --- | ---: | --- |\n")
	for _, m := range r.Members {
		detail := ""
		switch m.Status {
		case windowsFails:
			detail = markdownCell(formatWindowsError(m.Errors[0]))
		case windowsBlocked:
			detail = "blocked by " + markdownCell(strings.Join(m.Blockers, ", "))
		case windowsNotBuilt:
			detail = "cargo never reached it"
		}
		fmt.Fprintf(&sb, "| `%s` | %s | %d | %s |\n", m.Name, m.Status, len(m.Errors), detail)
	}

	var ready []string
	for _, m := range r.Members {
		if m.Status == windowsCompiles {
			ready = append(ready, "`"+m.Name+"`")
		}
	}
	if len(ready) > 0 {
		sb.WriteString("\nReady for the must-compile list (drop the `platforms` restriction from the manifest, then " +
			"`pnpm check member-coverage` shrink-wraps the allowlist): " + strings.Join(ready, ", ") + ".\n")
	}

	if len(r.FailedDeps) > 0 {
		sb.WriteString("\nThird-party dependencies that fail:\n\n")
		for _, name := range sortedKeys(r.FailedDeps) {
			fmt.Fprintf(&sb, "- `%s`: %s\n", name, markdownCell(r.FailedDeps[name]))
		}
	}

	for _, m := range r.Members {
		if len(m.Errors) == 0 {
			continue
		}
		shown := min(len(m.Errors), windowsBreaksFirstErrors)
		fmt.Fprintf(&sb, "\n### `%s`: %d %s\n\n", m.Name, len(m.Errors), Pluralize(len(m.Errors), "error", "errors"))
		for _, e := range m.Errors[:shown] {
			fmt.Fprintf(&sb, "- %s\n", markdownCell(formatWindowsError(e)))
		}
		if len(m.Errors) > shown {
			fmt.Fprintf(&sb, "- and %d more in `errors.md`\n", len(m.Errors)-shown)
		}
	}
	return sb.String()
}

// errorsMarkdown is every distinct error, rendered, grouped by member.
func (r windowsBreakReport) errorsMarkdown() string {
	var sb strings.Builder
	fmt.Fprintf(&sb, "# Windows compile errors on %s\n", r.where())
	for _, m := range r.Members {
		if len(m.Errors) == 0 {
			continue
		}
		fmt.Fprintf(&sb, "\n## `%s`\n\n```\n", m.Name)
		for _, e := range m.Errors {
			sb.WriteString(strings.TrimRight(e.Rendered, "\n"))
			sb.WriteString("\n\n")
		}
		sb.WriteString("```\n")
	}
	return sb.String()
}

func formatWindowsError(e windowsCompileError) string {
	if e.Location == "" {
		return e.Message
	}
	return e.Location + ": " + e.Message
}

// markdownCell keeps a value on one table row.
func markdownCell(s string) string {
	s = strings.ReplaceAll(s, "\n", " ")
	return strings.ReplaceAll(s, "|", "\\|")
}
