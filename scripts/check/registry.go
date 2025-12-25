package main

import "strings"

// getCheckByName returns a check by its CLI name.
func getCheckByName(name string) Check {
	nameLower := strings.ToLower(name)

	// Map CLI names (with dashes, case-insensitive) to check types
	switch nameLower {
	case "go-mod-tidy":
		return &GoModTidyCheck{}
	case "go-vet":
		return &GoVetCheck{}
	case "backend-tests":
		return &BackendTestsCheck{}
	case "frontend-tests":
		return &FrontendTestsCheck{}
	case "e2e-tests":
		return &E2ETestsCheck{}
	case "gocyclo":
		return &GocycloCheck{}
	case "nilaway":
		return &NilawayCheck{}
	case "hugo-build":
		return &HugoBuildCheck{}
	case "docs-links":
		return &DocsLinksCheck{}
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
	checks = append(checks, getBackendChecks()...)
	checks = append(checks, getFrontendChecks()...)
	checks = append(checks, getDocsChecks()...)
	return checks
}

// getBackendChecks returns all backend checks.
func getBackendChecks() []Check {
	return []Check{
		&GofmtCheck{},
		&GoModTidyCheck{},
		&GovulncheckCheck{},
		&GoVetCheck{},
		&StaticcheckCheck{},
		&IneffassignCheck{},
		&MisspellCheck{},
		&GocycloCheck{},
		&NilawayCheck{},
		&BackendTestsCheck{},
	}
}

// getFrontendChecks returns all frontend checks.
func getFrontendChecks() []Check {
	return []Check{
		&PrettierCheck{},
		&ESLintCheck{},
		&FrontendTestsCheck{},
		&E2ETestsCheck{},
	}
}

// getDocsChecks returns all docs checks.
func getDocsChecks() []Check {
	return []Check{
		&HugoBuildCheck{},
		&DocsLinksCheck{},
	}
}

// getCheckCLIName returns the CLI name for a check (for use in command suggestions).
func getCheckCLIName(check Check) string {
	name := strings.ToLower(check.Name())
	// Map check names to their CLI equivalents
	switch name {
	case "go mod tidy":
		return "go-mod-tidy"
	case "go vet":
		return "go-vet"
	case "tests":
		// Need to determine if it's backend or frontend - check the type
		switch check.(type) {
		case *BackendTestsCheck:
			return "backend-tests"
		case *FrontendTestsCheck:
			return "frontend-tests"
		}
		return "tests"
	case "e2e tests":
		return "e2e-tests"
	case "hugo build":
		return "hugo-build"
	case "docs links":
		return "docs-links"
	default:
		return name
	}
}
