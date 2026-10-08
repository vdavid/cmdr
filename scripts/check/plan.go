package main

import (
	"fmt"
	"os"
	"time"

	"cmdr/scripts/check/checks"
)

// The cache plan turns "affected-only selection" and "result caching" into one
// baseline-free mechanism: a check runs IFF its inputs changed since it last
// passed. Planning happens BEFORE Docker fixture bring-up and before the run, so
// a run where every container-touching check is a cache hit never starts one,
// and a fully-cached `pnpm check` is near-instant.
//
// Cache-awareness is the default. It's bypassed (everything runs fresh, and this
// run's passes overwrite the cache) when:
//   - --fresh is passed, or CMDR_CHECK_NO_CACHE=1 is set (debug escape hatch),
//   - --ci is set: CI is the authoritative backstop against a wrong Inputs list,
//     so it always runs fresh AND never writes the cache (mirrors --ci's
//     no-stats-logging behavior),
//   - a check was named explicitly (positional or --check): naming a check is the
//     existing "I want this to actually run" escape hatch, so named checks always
//     run fresh. Group/app selectors stay cache-aware.

// cachedHit pairs a cache-skipped check with the message to replay on its skip
// line, so the runner can report it with real context.
type cachedHit struct {
	def     checks.CheckDefinition
	message string
}

// cachePlan is the outcome of planning: which checks actually run, which were
// served from cache, and the fingerprints to record for this run's passes.
type cachePlan struct {
	toRun         []checks.CheckDefinition
	cached        []cachedHit
	fingerprints  map[string]string  // check ID → fingerprint, for every selected check
	active        bool               // false ⇒ this run made no cache-skip decisions
	writeDisabled bool               // true ⇒ recordRun persists nothing (--ci, or planning bailed)
	cache         *checks.CheckCache // always non-nil; recordRun honors writeDisabled
}

// cacheBypassed reports whether this run refuses to trust anything a previous run
// recorded: `--fresh` and `CMDR_CHECK_NO_CACHE` are the debug escape hatches,
// `--ci` runs fresh by contract. Naming checks is a separate, narrower rule that
// only the check-level cache applies (see `planCache`), because "I named this
// check" means "actually run it", not "rebuild its inputs from scratch".
func cacheBypassed(flags *cliFlags) bool {
	return flags.fresh || flags.ciMode || os.Getenv("CMDR_CHECK_NO_CACHE") != ""
}

// planCache splits the selected checks into cache hits and checks that must run.
// It never errors out the run: any failure to read git or the cache degrades to
// "run everything fresh" (active=false on the read side, but we still try to
// write fresh passes unless writing is disabled).
func planCache(ctx *checks.CheckContext, flags *cliFlags, selected []checks.CheckDefinition) *cachePlan {
	writeDisabled := flags.ciMode // CI never writes the cache
	cacheReads := !cacheBypassed(flags) && len(flags.checkNames) == 0

	plan := &cachePlan{
		toRun:         selected,
		fingerprints:  map[string]string{},
		active:        cacheReads,
		writeDisabled: writeDisabled,
		cache:         checks.LoadCheckCache(ctx.RootDir),
	}

	// Fingerprints are needed for both reading (skip decisions) and writing
	// (recording this run's passes). Skip the git pass only when neither applies.
	if !cacheReads && writeDisabled {
		return plan
	}

	data, err := checks.CollectRepoFingerprintData(ctx.RootDir)
	if err != nil {
		// Not a git tree, or git misbehaved: run everything, write nothing.
		plan.active = false
		plan.writeDisabled = true
		return plan
	}

	namedFresh := namedCheckIDs(flags.checkNames)
	for i := range selected {
		def := &selected[i]
		fp := data.FingerprintFor(def)
		plan.fingerprints[def.ID] = fp
	}

	if !cacheReads {
		return plan // fingerprints recorded for the write side; everything runs
	}

	var toRun []checks.CheckDefinition
	for i := range selected {
		def := selected[i]
		if namedFresh[def.ID] {
			toRun = append(toRun, def)
			continue
		}
		entry, ok := plan.cache.Entries[def.ID]
		if ok && entry.Fingerprint == plan.fingerprints[def.ID] {
			plan.cached = append(plan.cached, cachedHit{def: def, message: entry.Message})
			continue
		}
		toRun = append(toRun, def)
	}
	plan.toRun = toRun
	return plan
}

// namedCheckIDs resolves named selectors (positional or --check) to canonical
// check IDs so the lookup in planCache is by ID regardless of nickname.
func namedCheckIDs(names []string) map[string]bool {
	out := map[string]bool{}
	for _, n := range names {
		if c := checks.GetCheckByID(n); c != nil {
			out[c.ID] = true
		}
	}
	return out
}

// recordRun updates and persists the cache after a run: a passing check records
// its fingerprint, any other outcome (warn, skip, fail, block) drops a stale
// entry so it can't mask a later regression. Cache-hit entries carry forward
// untouched. Writing is skipped entirely when the cache is disabled (--ci).
//
// Two guards keep a recorded pass honest:
//   - A pass is recorded only if the check's inputs still fingerprint the same
//     at the end of the run. A file edited mid-run means the pass belongs to
//     content nobody fingerprinted; recording it under the plan-time key would
//     let an A→B→A edit hit the cache on content that never ran.
//   - The cache file is reloaded just before saving and this run's verdicts are
//     merged into it per check ID, so concurrent runs in one worktree neither
//     overwrite each other's passes nor resurrect an entry the other dropped.
func (plan *cachePlan) recordRun(rootDir string, states []*CheckState) {
	if plan.writeDisabled || plan.cache == nil {
		return // writes disabled (--ci) or planning bailed
	}
	// One more git pass, only when there's a pass to verify. If git fails now,
	// endData stays nil and every pass is treated as unverified (dropped).
	var endData *checks.RepoFingerprintData
	for _, st := range states {
		if isCacheablePass(st) {
			endData, _ = checks.CollectRepoFingerprintData(rootDir)
			break
		}
	}

	cache := checks.LoadCheckCache(rootDir)
	for _, st := range states {
		id := st.Definition.ID
		fp, hasFp := plan.fingerprints[id]
		if isCacheablePass(st) && hasFp && endData != nil && endData.FingerprintFor(st.Definition) == fp {
			cache.Entries[id] = checks.CacheEntry{
				Fingerprint: fp,
				Message:     st.Result.Message,
				PassedAt:    time.Now(),
			}
		} else {
			// Warn/skip/fail/block, or inputs changed mid-run: never cache. Drop any
			// stale pass entry.
			delete(cache.Entries, id)
		}
	}
	plan.cache = cache
	if err := cache.Save(rootDir); err != nil {
		// Non-fatal: a failed write just means the next run re-checks.
		fmt.Fprintf(os.Stderr, "%swarning: couldn't write check cache: %v%s\n", colorDim, err, colorReset)
	}
}

// isCacheablePass reports whether a check's outcome is one the cache records.
func isCacheablePass(st *CheckState) bool {
	return st.Status == StatusCompleted && st.Result.Code == checks.ResultSuccess
}
