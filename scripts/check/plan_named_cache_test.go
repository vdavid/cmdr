package main

import (
	"testing"

	"cmdr/scripts/check/checks"
)

func relatedCheck(t *testing.T) checks.CheckDefinition {
	t.Helper()
	related := checks.GetCheckByID("svelte-tests-related")
	if related == nil {
		t.Fatal("expected svelte-tests-related in the registry")
	}
	if !related.CachedWhenNamed {
		t.Fatal("svelte-tests-related must opt into CachedWhenNamed")
	}
	return *related
}

// A lane that opts in with `CachedWhenNamed` reuses an identical pass even when
// named: the pre-commit ritual names `svelte-tests-related` every time, and a
// repeat on unchanged inputs and an unchanged branch base would only re-run it.
func TestPlanNamedCheckCachedWhenNamedHitsCache(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"apps/desktop/src/a.ts": "export const a = 1"})
	ctx := &checks.CheckContext{RootDir: dir}
	related := relatedCheck(t)
	seedCache(t, ctx, related)
	named := []string{"svelte-tests-related"}

	plan := planCache(ctx, &cliFlags{checkNames: named}, []checks.CheckDefinition{related})
	if len(plan.toRun) != 0 || len(plan.cached) != 1 {
		t.Fatalf("a CachedWhenNamed check must reuse its pass when named, got toRun=%v cached=%v", plan.toRun, plan.cached)
	}

	plan = planCache(ctx, &cliFlags{checkNames: named, fresh: true}, []checks.CheckDefinition{related})
	if len(plan.toRun) != 1 {
		t.Fatalf("--fresh must still force a named CachedWhenNamed check to run, got toRun=%v", plan.toRun)
	}

	writeFile(t, dir, "apps/desktop/src/a.ts", "export const a = 2")
	plan = planCache(ctx, &cliFlags{checkNames: named}, []checks.CheckDefinition{related})
	if len(plan.toRun) != 1 {
		t.Fatalf("a changed input must re-run a named CachedWhenNamed check, got toRun=%v", plan.toRun)
	}
}

// Opting one named lane into the cache leaves the rest of a named run as it was:
// everything else in it (the scoped doc lanes, other named checks) runs fresh.
func TestPlanNamedRunKeepsOtherChecksFresh(t *testing.T) {
	dir := planGitRepo(t, map[string]string{"a.txt": "v1"})
	ctx := &checks.CheckContext{RootDir: dir}
	other := noopCheck("c1", "a.txt")
	seedCache(t, ctx, other)

	plan := planCache(ctx, &cliFlags{checkNames: []string{"svelte-tests-related"}}, []checks.CheckDefinition{relatedCheck(t), other})
	for _, hit := range plan.cached {
		if hit.def.ID == "c1" {
			t.Fatal("a check riding along in a named run must run fresh")
		}
	}
}
