package main

import "strings"

// getCheckByName returns a check by its CLI name.
func getCheckByName(name string) Check {
	nameLower := strings.ToLower(name)

	// Map CLI names (with dashes, case-insensitive) to check types
	switch nameLower {
	case "rustfmt":
		return &RustfmtCheck{}
	case "clippy":
		return &ClippyCheck{}
	case "cargo-audit":
		return &CargoAuditCheck{}
	case "cargo-deny":
		return &CargoDenyCheck{}
	case "cargo-udeps":
		return &CargoUdepsCheck{}
	case "rust-tests":
		return &RustTestsCheck{}
	case "prettier":
		return &PrettierCheck{}
	case "eslint":
		return &ESLintCheck{}
	case "svelte-check":
		return &SvelteCheck{}
	case "knip":
		return &KnipCheck{}
	case "svelte-tests":
		return &SvelteTestsCheck{}
	case "e2e-tests":
		return &E2ETestsCheck{}
	default:
		// Try to find by exact name match (case-insensitive)
		allChecks := getAllChecks()
		for _, check := range allChecks {
			checkNameLower := strings.ToLower(check.Name())
			if checkNameLower == nameLower {
				return check
			}
		}
		return nil
	}
}

// getAllChecks returns all available checks.
func getAllChecks() []Check {
	var checks []Check
	checks = append(checks, getRustChecks()...)
	checks = append(checks, getSvelteChecks()...)
	return checks
}

// getRustChecks returns all Rust checks.
func getRustChecks() []Check {
	return []Check{
		&RustfmtCheck{},
		&ClippyCheck{},
		&CargoAuditCheck{},
		&CargoDenyCheck{},
		&CargoUdepsCheck{},
		&RustTestsCheck{},
	}
}

// getSvelteChecks returns all Svelte checks.
func getSvelteChecks() []Check {
	return []Check{
		&PrettierCheck{},
		&ESLintCheck{},
		&SvelteCheck{},
		&KnipCheck{},
		&SvelteTestsCheck{},
		&E2ETestsCheck{},
	}
}

// getCheckCLIName returns the CLI name for a check (for use in command suggestions).
func getCheckCLIName(check Check) string {
	name := strings.ToLower(check.Name())
	// Map check names to their CLI equivalents
	switch name {
	case "tests":
		// Determine if it's Rust or Svelte
		switch check.(type) {
		case *RustTestsCheck:
			return "rust-tests"
		case *SvelteTestsCheck:
			return "svelte-tests"
		}
		return "tests"
	case "e2e tests":
		return "e2e-tests"
	default:
		return name
	}
}
