package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"cmdr/scripts/check/checks"
)

// planGitRepo creates a throwaway git repo with the given files committed.
func planGitRepo(t *testing.T, files map[string]string) string {
	t.Helper()
	dir := t.TempDir()
	git := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(),
			"GIT_AUTHOR_NAME=Test", "GIT_AUTHOR_EMAIL=t@e.com",
			"GIT_COMMITTER_NAME=Test", "GIT_COMMITTER_EMAIL=t@e.com")
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	git("init", "-q")
	for path, content := range files {
		abs := filepath.Join(dir, path)
		if err := os.MkdirAll(filepath.Dir(abs), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(abs, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	git("add", "-A")
	git("commit", "-q", "-m", "init")
	return dir
}

func writeFile(t *testing.T, dir, rel, content string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, rel), []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

// noopCheck builds a minimal check definition scoped to the given input glob.
func noopCheck(id string, inputs ...string) checks.CheckDefinition {
	return checks.CheckDefinition{ID: id, DisplayName: id, App: checks.AppOther, Inputs: inputs}
}

// seedCache writes a passing cache entry for id with the current fingerprint.
func seedCache(t *testing.T, ctx *checks.CheckContext, def checks.CheckDefinition) {
	t.Helper()
	data, err := checks.CollectRepoFingerprintData(ctx.RootDir)
	if err != nil {
		t.Fatal(err)
	}
	c := checks.LoadCheckCache(ctx.RootDir)
	c.Entries[def.ID] = checks.CacheEntry{Fingerprint: data.FingerprintFor(&def), Message: "passed"}
	if err := c.Save(ctx.RootDir); err != nil {
		t.Fatal(err)
	}
}

func TestPlanCacheHitSkipsCheck(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"a.txt": "v1"})
	ctx := &checks.CheckContext{RootDir: dir}
	def := noopCheck("c1", "a.txt")
	seedCache(t, ctx, def)

	plan := planCache(ctx, &cliFlags{}, []checks.CheckDefinition{def})
	if len(plan.toRun) != 0 {
		t.Fatalf("unchanged inputs should skip the check, got toRun=%v", plan.toRun)
	}
	if len(plan.cached) != 1 || plan.cached[0].def.ID != "c1" {
		t.Fatalf("expected c1 served from cache, got %+v", plan.cached)
	}
}

func TestPlanInputChangeRerun(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"a.txt": "v1"})
	ctx := &checks.CheckContext{RootDir: dir}
	def := noopCheck("c1", "a.txt")
	seedCache(t, ctx, def)

	writeFile(t, dir, "a.txt", "v2-changed") // input changed since cached pass
	plan := planCache(ctx, &cliFlags{}, []checks.CheckDefinition{def})
	if len(plan.toRun) != 1 {
		t.Fatalf("changed input must re-run the check, got toRun=%v cached=%v", plan.toRun, plan.cached)
	}
	if len(plan.cached) != 0 {
		t.Fatal("changed input must not be a cache hit")
	}
}

func TestPlanNamedCheckBypassesCache(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"apps/desktop/src-tauri/x.rs": "fn x() {}"})
	ctx := &checks.CheckContext{RootDir: dir}
	// Use a real registry check so namedCheckIDs (GetCheckByID) resolves it.
	clippy := checks.GetCheckByID("clippy")
	if clippy == nil {
		t.Fatal("expected clippy in the registry")
	}
	seedCache(t, ctx, *clippy)

	plan := planCache(ctx, &cliFlags{checkNames: []string{"clippy"}}, []checks.CheckDefinition{*clippy})
	if len(plan.toRun) != 1 {
		t.Fatalf("a named check must always run fresh, got toRun=%v cached=%v", plan.toRun, plan.cached)
	}
}

func TestPlanFreshBypassesCache(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"a.txt": "v1"})
	ctx := &checks.CheckContext{RootDir: dir}
	def := noopCheck("c1", "a.txt")
	seedCache(t, ctx, def)

	plan := planCache(ctx, &cliFlags{fresh: true}, []checks.CheckDefinition{def})
	if len(plan.toRun) != 1 || len(plan.cached) != 0 {
		t.Fatalf("--fresh must run everything, got toRun=%v cached=%v", plan.toRun, plan.cached)
	}
	if plan.writeDisabled {
		t.Fatal("--fresh still writes the cache (refreshes entries)")
	}
}

func TestPlanCINeverReadsOrWritesCache(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"a.txt": "v1"})
	ctx := &checks.CheckContext{RootDir: dir, CI: true}
	def := noopCheck("c1", "a.txt")
	seedCache(t, ctx, def)

	plan := planCache(ctx, &cliFlags{ciMode: true}, []checks.CheckDefinition{def})
	if len(plan.toRun) != 1 {
		t.Fatalf("--ci must run everything fresh, got toRun=%v", plan.toRun)
	}
	if !plan.writeDisabled {
		t.Fatal("--ci must never write the cache")
	}
}

func TestPlanEnvNoCacheBypasses(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"a.txt": "v1"})
	ctx := &checks.CheckContext{RootDir: dir}
	def := noopCheck("c1", "a.txt")
	seedCache(t, ctx, def)

	t.Setenv("CMDR_CHECK_NO_CACHE", "1")
	plan := planCache(ctx, &cliFlags{}, []checks.CheckDefinition{def})
	if len(plan.toRun) != 1 || len(plan.cached) != 0 {
		t.Fatalf("CMDR_CHECK_NO_CACHE=1 must bypass cache reads, got toRun=%v cached=%v", plan.toRun, plan.cached)
	}
}

func TestPlanCorruptCacheTolerated(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"a.txt": "v1"})
	ctx := &checks.CheckContext{RootDir: dir}
	cachePath := filepath.Join(dir, checks.CheckCachePath)
	if err := os.MkdirAll(filepath.Dir(cachePath), 0o755); err != nil {
		t.Fatal(err)
	}
	writeFile(t, dir, checks.CheckCachePath, "{garbage")

	def := noopCheck("c1", "a.txt")
	plan := planCache(ctx, &cliFlags{}, []checks.CheckDefinition{def})
	if len(plan.toRun) != 1 {
		t.Fatalf("corrupt cache must degrade to running everything, got toRun=%v", plan.toRun)
	}
}

func TestPlanNonGitTreeRunsEverything(t *testing.T) {
	dir := t.TempDir() // no git
	ctx := &checks.CheckContext{RootDir: dir}
	def := noopCheck("c1", "a.txt")
	plan := planCache(ctx, &cliFlags{}, []checks.CheckDefinition{def})
	if len(plan.toRun) != 1 || plan.active {
		t.Fatalf("non-git tree must run everything with cache inactive, got toRun=%v active=%v", plan.toRun, plan.active)
	}
}

// TestPlanSmbSkippedWhenAllCached verifies the planning-level outcome that lets
// the runner skip SMB bring-up: when every NeedsSmb check is a cache hit, none
// remain in toRun, so setupSmbOrchestratorIfNeeded sees no SMB modes.
func TestPlanSmbSkippedWhenAllCached(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"smb/conf": "x"})
	ctx := &checks.CheckContext{RootDir: dir}
	def := noopCheck("smb-check", "smb/**")
	def.NeedsSmb = checks.SmbModeCore
	seedCache(t, ctx, def)

	plan := planCache(ctx, &cliFlags{}, []checks.CheckDefinition{def})
	if len(plan.toRun) != 0 {
		t.Fatalf("cached SMB check must not be in toRun, got %v", plan.toRun)
	}
	if modes := collectModes(plan.toRun); len(modes) != 0 {
		t.Fatalf("no SMB modes should remain when all SMB checks are cached, got %v", modes)
	}
}

func TestRecordRunCachesPassDropsFail(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"a.txt": "v1", "b.txt": "v1"})
	ctx := &checks.CheckContext{RootDir: dir}
	passing := noopCheck("pass", "a.txt")
	failing := noopCheck("fail", "b.txt")
	// Pre-seed a stale entry for the failing check to prove it gets dropped.
	seedCache(t, ctx, failing)

	plan := planCache(ctx, &cliFlags{fresh: true}, []checks.CheckDefinition{passing, failing})

	states := []*CheckState{
		{Definition: &passing, Status: StatusCompleted, Result: checks.Success("ok")},
		{Definition: &failing, Status: StatusFailed},
	}
	plan.recordRun(dir, states)

	reloaded := checks.LoadCheckCache(dir)
	if _, ok := reloaded.Entries["pass"]; !ok {
		t.Fatal("a passing check must be cached after the run")
	}
	if _, ok := reloaded.Entries["fail"]; ok {
		t.Fatal("a failing check must drop its (stale) cache entry")
	}
}

func TestRecordRunDropsWarn(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"a.txt": "v1"})
	plan := &cachePlan{
		cache:        checks.LoadCheckCache(dir),
		fingerprints: map[string]string{"warn": "fp"},
	}
	def := noopCheck("warn", "a.txt")
	warnResult := checks.Success("heads up")
	warnResult.Code = checks.ResultWarning
	states := []*CheckState{{Definition: &def, Status: StatusCompleted, Result: warnResult}}
	plan.recordRun(dir, states)

	if _, ok := checks.LoadCheckCache(dir).Entries["warn"]; ok {
		t.Fatal("warn results must not be cached (their messages are the product)")
	}
}
