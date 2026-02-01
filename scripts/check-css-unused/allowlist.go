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
}

// allowedUnusedVariables lists CSS custom properties that are defined but used dynamically,
// or defined for future use / theming purposes.
var allowedUnusedVariables = map[string]bool{
	// Example: "color-future-feature": true, // Reserved for upcoming feature X
}

// allowedUndefinedClasses lists classes used in templates that don't need CSS definitions
// (used for JS selection, third-party libs, or semantic purposes).
var allowedUndefinedClasses = map[string]bool{
	// Ark UI component class passed for API purposes but not styled
	"slider-root": true,
}
