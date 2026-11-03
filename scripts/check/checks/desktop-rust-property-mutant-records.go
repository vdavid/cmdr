package checks

import (
	"fmt"
	"path/filepath"
	"strings"
)

// RunPropertyMutantRecords is the cheap half of the property-mutant pair: no
// compiler, only the records against the tree. It fails when a property has
// neither a record nor an allowlist entry, and when a record no longer applies
// (its `find` is gone or ambiguous, its property or input vanished), so a
// refactor can't strand a record until the slow lane notices days later.
// `desktop-rust-property-mutants` is the half that runs them.
func RunPropertyMutantRecords(ctx *CheckContext) (CheckResult, error) {
	return checkPropertyMutantRecords(ctx, propertyKinds)
}

func checkPropertyMutantRecords(ctx *CheckContext, kinds []propertyKind) (CheckResult, error) {
	records, problems, err := loadMutantRecords(ctx.RootDir, kinds)
	if err != nil {
		return CheckResult{}, err
	}
	properties, err := discoverProperties(ctx.RootDir, kinds)
	if err != nil {
		return CheckResult{}, err
	}
	allowlist, err := loadPropertyMutantsAllowlist(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	coverage := judgePropertyCoverage(properties, records, &allowlist)

	madeChanges := false
	if len(coverage.stale) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(filepath.Join(ctx.RootDir, propertyMutantsAllowlistRelPath), allowlist); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, propertyMutantsAllowlistRelPath)
		madeChanges = true
	}
	staleMsg := formatLockPoisonStaleness(coverage.stale, ctx.CI)

	var parts []string
	if len(problems) > 0 {
		parts = append(parts, fmt.Sprintf("%d mutant %s no longer %s (`scripts/check/checks/DETAILS.md` § \"Property mutants\"):\n  - %s",
			len(problems), Pluralize(len(problems), "record", "records"), Pluralize(len(problems), "applies", "apply"),
			strings.Join(problems, "\n  - ")))
	}
	if len(coverage.unrecorded) > 0 {
		parts = append(parts, fmt.Sprintf("%d %s no failing mutant on record, so nothing shows %s can fail. "+
			"Record a one-line change that breaks it in a `*%s` beside it (`scripts/check/checks/DETAILS.md` § \"Property mutants\"):\n  - %s",
			len(coverage.unrecorded), Pluralize(len(coverage.unrecorded), "property has", "properties have"),
			Pluralize(len(coverage.unrecorded), "it", "they"), mutantRecordSuffix, strings.Join(coverage.unrecorded, "\n  - ")))
	}
	if len(coverage.reasonless) > 0 {
		parts = append(parts, fmt.Sprintf("%d allowlist %s no `reason` in %s:\n  - %s",
			len(coverage.reasonless), Pluralize(len(coverage.reasonless), "entry has", "entries have"),
			propertyMutantsAllowlistRelPath, strings.Join(coverage.reasonless, "\n  - ")))
	}
	if len(parts) > 0 {
		parts = appendNonEmpty(parts, staleMsg)
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}

	msg := fmt.Sprintf("%d %s, %d mutant %s, all applying",
		len(properties), Pluralize(len(properties), "property", "properties"),
		len(records), Pluralize(len(records), "record", "records"))
	if n := len(allowlist.Unrecorded); n > 0 {
		msg += fmt.Sprintf(" (%d %s allowlisted)", n, Pluralize(n, "property", "properties"))
	}
	if staleMsg != "" {
		msg += "; " + staleMsg
	}
	if madeChanges {
		return SuccessWithChanges(msg), nil
	}
	return Success(msg), nil
}
