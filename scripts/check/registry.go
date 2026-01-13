package main

import "strings"

// getCheckByName returns a check by its CLI name.
func getCheckByName(name string) Check {
	nameLower := strings.ToLower(name)

	// Map CLI names (with dashes, case-insensitive) to check types
	switch nameLower {
	// Rust checks
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
	case "rust-tests-linux":
		return &RustTestsLinuxCheck{}
	// Desktop/Svelte checks
	case "prettier":
		return &PrettierCheck{}
	case "eslint":
		return &ESLintCheck{}
	case "stylelint":
		return &StylelintCheck{}
	case "svelte-check":
		return &SvelteCheck{}
	case "knip":
		return &KnipCheck{}
	case "svelte-tests":
		return &SvelteTestsCheck{}
	case "e2e-tests":
		return &E2ETestsCheck{}
	// Website checks
	case "website-prettier":
		return &WebsitePrettierCheck{}
	case "website-eslint":
		return &WebsiteESLintCheck{}
	case "website-typecheck":
		return &WebsiteTypecheckCheck{}
	case "website-build":
		return &WebsiteBuildCheck{}
	case "website-e2e-tests":
		return &WebsiteE2ETestsCheck{}
	// License server checks
	case "license-server-prettier":
		return &LicenseServerPrettierCheck{}
	case "license-server-eslint":
		return &LicenseServerESLintCheck{}
	case "license-server-typecheck":
		return &LicenseServerTypecheckCheck{}
	case "license-server-tests":
		return &LicenseServerTestsCheck{}
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
	checks = append(checks, getWebsiteChecks()...)
	checks = append(checks, getLicenseServerChecks()...)
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
		&RustTestsLinuxCheck{},
	}
}

// getSvelteChecks returns all Svelte/desktop checks.
func getSvelteChecks() []Check {
	return []Check{
		&PrettierCheck{},
		&ESLintCheck{},
		&StylelintCheck{},
		&SvelteCheck{},
		&KnipCheck{},
		&SvelteTestsCheck{},
		&E2ETestsCheck{},
	}
}

// getWebsiteChecks returns all website checks.
func getWebsiteChecks() []Check {
	return []Check{
		&WebsitePrettierCheck{},
		&WebsiteESLintCheck{},
		&WebsiteTypecheckCheck{},
		&WebsiteBuildCheck{},
		&WebsiteE2ETestsCheck{},
	}
}

// getLicenseServerChecks returns all license server checks.
func getLicenseServerChecks() []Check {
	return []Check{
		&LicenseServerPrettierCheck{},
		&LicenseServerESLintCheck{},
		&LicenseServerTypecheckCheck{},
		&LicenseServerTestsCheck{},
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
	case "tests (linux)":
		return "rust-tests-linux"
	case "e2e tests":
		return "e2e-tests"
	case "prettier (website)":
		return "website-prettier"
	case "eslint (website)":
		return "website-eslint"
	case "typecheck (website)":
		return "website-typecheck"
	case "build (website)":
		return "website-build"
	case "e2e tests (website)":
		return "website-e2e-tests"
	case "prettier (license-server)":
		return "license-server-prettier"
	case "eslint (license-server)":
		return "license-server-eslint"
	case "typecheck (license-server)":
		return "license-server-typecheck"
	case "tests (license-server)":
		return "license-server-tests"
	default:
		return name
	}
}
