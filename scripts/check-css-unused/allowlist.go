package main

// Allowlists for CSS classes and variables that can't be detected by static analysis.
// Add entries here with a comment explaining why they're needed.

// allowedUnusedClasses lists CSS classes that are defined but used dynamically
// (constructed at runtime, used in third-party libs, or referenced via string interpolation).
var allowedUnusedClasses = map[string]bool{
	// Size tier classes - applied dynamically via triad.tierClass in FullList.svelte and SelectionInfo.svelte
	"size-bytes": true,
	"size-kb":    true,
	"size-mb":    true,
	"size-gb":    true,
	"size-tb":    true,
	// SettingSelect.svelte - classes used with :global() for Ark UI Select component styling
	"custom-highlighted": true,
	"select-content":     true,
	// DualPaneExplorer.svelte - applied imperatively via classList.add during drag-and-drop
	"folder-drop-target": true,
	// Button.svelte - classes constructed dynamically via template strings (btn-{variant}, btn-{size})
	"btn-primary":   true,
	"btn-secondary": true,
	"btn-danger":    true,
	"btn-mini":      true,
	"btn-regular":   true,
	// Tooltip - singleton DOM node created/managed by tooltip.ts action, not in Svelte templates
	"cmdr-tooltip":     true,
	"cmdr-tooltip-kbd": true,
	"visible":          true,
}

// allowedUnusedVariables lists CSS custom properties that are defined but used dynamically,
// or defined for future use / theming purposes.
var allowedUnusedVariables = map[string]bool{
	// Design system tokens defined but not yet consumed by components
	"z-base":    true,
	"z-overlay": true,
	"z-sticky":  true,
	// Disk usage bar colors - referenced via dynamic inline styles (constructed CSS var names in JS)
	"color-disk-ok":      true,
	"color-disk-warning": true,
	"color-disk-danger":  true,
}

// allowedUndefinedClasses lists classes used in templates that don't need CSS definitions
// (used for JS selection, third-party libs, or semantic purposes).
var allowedUndefinedClasses = map[string]bool{
	// Ark UI component class passed for API purposes but not styled
	"slider-root": true,
}
