package main

import (
	"errors"
	"flag"
	"slices"
	"testing"

	"cmdr/scripts/check/checks"
)

func TestParseFlags_PositionalCheckNames(t *testing.T) {
	flags, err := parseFlags([]string{"oxfmt", "website-typecheck", "website-build"})
	if err != nil {
		t.Fatalf("parseFlags() returned error: %v", err)
	}
	want := []string{"oxfmt", "website-typecheck", "website-build"}
	if !slices.Equal(flags.checkNames, want) {
		t.Errorf("checkNames = %v, want %v", flags.checkNames, want)
	}
	if !flags.includeSlow {
		t.Error("named checks should implicitly include slow checks (same as --check)")
	}
}

func TestParseFlags_PositionalCommaSeparated(t *testing.T) {
	flags, err := parseFlags([]string{"oxfmt,clippy", "rustfmt"})
	if err != nil {
		t.Fatalf("parseFlags() returned error: %v", err)
	}
	want := []string{"oxfmt", "clippy", "rustfmt"}
	if !slices.Equal(flags.checkNames, want) {
		t.Errorf("checkNames = %v, want %v", flags.checkNames, want)
	}
}

func TestParseFlags_PositionalGroups(t *testing.T) {
	flags, err := parseFlags([]string{"rust", "svelte", "go", "website", "api-server"})
	if err != nil {
		t.Fatalf("parseFlags() returned error: %v", err)
	}
	if !flags.rustOnly || !flags.svelteOnly || !flags.goOnly {
		t.Errorf("group keywords should set the matching group fields: rust=%v svelte=%v go=%v",
			flags.rustOnly, flags.svelteOnly, flags.goOnly)
	}
	wantApps := []string{"website", "api-server"}
	if !slices.Equal(flags.appNames, wantApps) {
		t.Errorf("appNames = %v, want %v", flags.appNames, wantApps)
	}
	if len(flags.checkNames) != 0 {
		t.Errorf("group keywords must not land in checkNames, got %v", flags.checkNames)
	}
	if flags.includeSlow {
		t.Error("group selectors must keep the default lanes (no implicit include-slow)")
	}
}

func TestParseFlags_FlagsAfterPositionals(t *testing.T) {
	flags, err := parseFlags([]string{"clippy", "--verbose", "oxfmt", "--fail-fast"})
	if err != nil {
		t.Fatalf("parseFlags() returned error: %v", err)
	}
	if !flags.verbose || !flags.failFast {
		t.Errorf("flags after positionals should parse: verbose=%v failFast=%v", flags.verbose, flags.failFast)
	}
	want := []string{"clippy", "oxfmt"}
	if !slices.Equal(flags.checkNames, want) {
		t.Errorf("checkNames = %v, want %v", flags.checkNames, want)
	}
}

func TestParseFlags_Quiet(t *testing.T) {
	for _, arg := range []string{"--quiet", "-q"} {
		flags, err := parseFlags([]string{arg, "clippy"})
		if err != nil {
			t.Fatalf("parseFlags(%q) returned error: %v", arg, err)
		}
		if !flags.quiet {
			t.Errorf("parseFlags(%q): quiet = false, want true", arg)
		}
	}

	flags, err := parseFlags([]string{"clippy"})
	if err != nil {
		t.Fatalf("parseFlags() returned error: %v", err)
	}
	if flags.quiet {
		t.Error("quiet should default to false without --quiet/-q")
	}
}

func TestSummarizeRun(t *testing.T) {
	mk := func(status CheckStatus, code checks.ResultCode) *CheckState {
		return &CheckState{Status: status, Result: checks.CheckResult{Code: code}}
	}
	r := &Runner{
		checks: []*CheckState{
			mk(StatusCompleted, checks.ResultSuccess), // OK
			mk(StatusCompleted, checks.ResultSuccess), // OK (a change-making pass still counts as OK)
			mk(StatusCompleted, checks.ResultWarning), // warn
			mk(StatusSkipped, checks.ResultSkipped),   // skipped
			mk(StatusFailed, checks.ResultSuccess),    // failed: counted in none
		},
		cached: []*CheckState{
			mk(StatusCached, checks.ResultSuccess),
			mk(StatusCached, checks.ResultSuccess),
		},
	}

	ok, warn, skipped := summarizeRun(r)
	if ok != 4 { // 2 completed-OK + 2 cached
		t.Errorf("ok = %d, want 4", ok)
	}
	if warn != 1 {
		t.Errorf("warn = %d, want 1", warn)
	}
	if skipped != 1 {
		t.Errorf("skipped = %d, want 1", skipped)
	}
}

func TestParseFlags_UnknownPositional(t *testing.T) {
	_, err := parseFlags([]string{"bogus-check"})
	if err == nil {
		t.Fatal("parseFlags() should error on an unknown positional name")
	}
}

func TestParseFlags_UnknownFlag(t *testing.T) {
	_, err := parseFlags([]string{"--bogus-flag"})
	if err == nil {
		t.Fatal("parseFlags() should error on an unknown flag")
	}
}

func TestParseFlags_CheckFlagStillWorks(t *testing.T) {
	flags, err := parseFlags([]string{"--check", "clippy", "--check", "oxfmt,rustfmt"})
	if err != nil {
		t.Fatalf("parseFlags() returned error: %v", err)
	}
	want := []string{"clippy", "oxfmt", "rustfmt"}
	if !slices.Equal(flags.checkNames, want) {
		t.Errorf("checkNames = %v, want %v", flags.checkNames, want)
	}
	if !flags.includeSlow {
		t.Error("--check should implicitly include slow checks")
	}
}

func TestParseFlags_AppFlagRepeatableAndCommaSeparated(t *testing.T) {
	flags, err := parseFlags([]string{"--app", "website,scripts", "--app", "desktop"})
	if err != nil {
		t.Fatalf("parseFlags() returned error: %v", err)
	}
	want := []string{"website", "scripts", "desktop"}
	if !slices.Equal(flags.appNames, want) {
		t.Errorf("appNames = %v, want %v", flags.appNames, want)
	}
}

func TestParseFlags_FastMutuallyExclusiveWithSlowLanes(t *testing.T) {
	for _, args := range [][]string{
		{"--fast", "--include-slow"},
		{"--fast", "--only-slow"},
	} {
		if _, err := parseFlags(args); err == nil {
			t.Errorf("parseFlags(%v) should error: --fast is mutually exclusive with the slow lanes", args)
		}
	}
}

func TestParseFlags_HelpReturnsErrHelp(t *testing.T) {
	for _, args := range [][]string{{"--help"}, {"-h"}} {
		_, err := parseFlags(args)
		if !errors.Is(err, flag.ErrHelp) {
			t.Errorf("parseFlags(%v) = %v, want flag.ErrHelp", args, err)
		}
	}
}

// Drift guard: every reserved selector keyword must classify as a group/app
// (never as an unknown name, never as a check). If this fails, the
// applySelector switch and reservedSelectorNames went out of sync.
func TestApplySelector_AllReservedNamesResolve(t *testing.T) {
	for _, name := range reservedSelectorNames {
		flags := &cliFlags{}
		if err := applySelector(flags, name); err != nil {
			t.Errorf("applySelector(%q) returned error: %v", name, err)
		}
		if len(flags.checkNames) != 0 {
			t.Errorf("applySelector(%q) classified a reserved group keyword as a check name", name)
		}
	}
}

func TestSelectChecks_MultipleAppsDeduped(t *testing.T) {
	flags := &cliFlags{appNames: []string{"website", "website", "scripts"}}
	selected, err := selectChecks(flags)
	if err != nil {
		t.Fatalf("selectChecks() returned error: %v", err)
	}
	seen := make(map[string]bool)
	hasWebsite, hasScripts := false, false
	for _, c := range selected {
		if seen[c.ID] {
			t.Errorf("check %q selected twice", c.ID)
		}
		seen[c.ID] = true
		switch c.CLIName() {
		case "website-build":
			hasWebsite = true
		case "go-tests":
			hasScripts = true
		}
	}
	if !hasWebsite || !hasScripts {
		t.Errorf("expected checks from both apps, got website=%v scripts=%v", hasWebsite, hasScripts)
	}
}

func TestSelectChecks_PositionalGroupPlusCheckName(t *testing.T) {
	flags := &cliFlags{rustOnly: true, checkNames: []string{"oxfmt"}}
	selected, err := selectChecks(flags)
	if err != nil {
		t.Fatalf("selectChecks() returned error: %v", err)
	}
	hasOxfmt, hasClippy := false, false
	for _, c := range selected {
		switch c.CLIName() {
		case "oxfmt":
			hasOxfmt = true
		case "clippy":
			hasClippy = true
		}
	}
	if !hasOxfmt || !hasClippy {
		t.Errorf("additive selection broke: oxfmt=%v clippy=%v", hasOxfmt, hasClippy)
	}
}
