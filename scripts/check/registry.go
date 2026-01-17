package main

import (
	"strings"

	"vmail/scripts/check/checks"
)

// getCheckByID returns a check definition by its ID (case-insensitive).
func getCheckByID(id string) *checks.CheckDefinition {
	idLower := strings.ToLower(id)
	for i := range checks.AllChecks {
		if strings.ToLower(checks.AllChecks[i].ID) == idLower {
			return &checks.AllChecks[i]
		}
	}
	return nil
}

// getAllChecks returns all check definitions.
func getAllChecks() []checks.CheckDefinition {
	return checks.AllChecks
}

// getChecksByApp returns all checks for a specific app.
func getChecksByApp(app checks.App) []checks.CheckDefinition {
	return checks.GetChecksByApp(app)
}

// getChecksByTech returns all checks matching app and tech.
func getChecksByTech(app checks.App, tech string) []checks.CheckDefinition {
	return checks.GetChecksByTech(app, tech)
}

// filterSlowChecks removes slow checks unless includeSlow is true.
func filterSlowChecks(defs []checks.CheckDefinition, includeSlow bool) []checks.CheckDefinition {
	if includeSlow {
		return defs
	}
	var result []checks.CheckDefinition
	for _, def := range defs {
		if !def.IsSlow {
			result = append(result, def)
		}
	}
	return result
}

// appDisplayName returns a human-readable name for an app with icon.
func appDisplayName(app checks.App) string {
	switch app {
	case checks.AppDesktop:
		return "🖥️  Desktop"
	case checks.AppWebsite:
		return "🌐 Website"
	case checks.AppLicenseServer:
		return "🔑 License server"
	default:
		return string(app)
	}
}
