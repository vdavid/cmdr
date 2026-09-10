package main

// Toast text on the level-tinted toast surfaces.
//
// `ToastItem.svelte` paints each level's surface on `.toast.<level>`, but the text roles the
// frame owns set their color on other selectors (`.toast-message`, `.toast-age`), so the rule
// walker never pairs the two. Here every level's surface meets both roles, in both modes. A
// toast content component's own text sits on the same surfaces: one that picks a quieter
// token than these needs its own entry.
var toastScenarios = buildToastScenarios()

func buildToastScenarios() []ancestorBgScenario {
	var out []ancestorBgScenario
	for _, level := range []string{"default", "info", "success", "warn", "error"} {
		bg := "color-toast-" + level + "-bg"
		out = append(out,
			ancestorBgScenario{Selector: ".toast (" + level + ") .toast-message", FgVar: "color-text-primary", BgVar: bg},
			ancestorBgScenario{Selector: ".toast (" + level + ") .toast-age", FgVar: "color-text-tertiary", BgVar: bg},
		)
	}
	return out
}

// AnalyzeToastStates evaluates each toast scenario in both modes, returning the worst-case
// finding per (selector, mode).
func (a *Analyzer) AnalyzeToastStates() []Finding {
	return a.analyzeAncestorBgScenarios(toastScenarios, "scripts/check-a11y-contrast/toast_states.go")
}
