package main

import (
	"regexp"
	"strings"
)

// Regex patterns for CSS parsing
var (
	// Matches CSS custom property definitions: --property-name: value
	cssVarDefRegex = regexp.MustCompile(`--([a-zA-Z][a-zA-Z0-9-]*)\s*:`)

	// Matches CSS custom property usage: var(--property-name)
	cssVarUseRegex = regexp.MustCompile(`var\(--([a-zA-Z][a-zA-Z0-9-]*)\)`)

	// Matches class names in CSS selectors (handles compound selectors like .parent.child)
	// Captures any .class-name pattern in CSS
	cssClassDefRegex = regexp.MustCompile(`\.([a-zA-Z_][a-zA-Z0-9_-]*)`)

	// Matches dynamic class binding in Svelte: class:name={...} or class:name
	classDynamicRegex = regexp.MustCompile(`class:([a-zA-Z_][a-zA-Z0-9_-]*)`)
)

// reservedCssNames contains pseudo-classes/elements that look like class names
var reservedCssNames = map[string]bool{
	"root": true, "before": true, "after": true, "hover": true, "focus": true,
	"active": true, "first": true, "last": true, "nth": true, "not": true,
	"global": true, "checked": true, "disabled": true, "empty": true,
	"enabled": true, "visited": true, "link": true, "target": true,
}

// extractStyleSection extracts content between <style> and </style> tags.
func extractStyleSection(svelteContent string) string {
	startIdx := strings.Index(svelteContent, "<style")
	if startIdx == -1 {
		return ""
	}
	tagEndIdx := strings.Index(svelteContent[startIdx:], ">")
	if tagEndIdx == -1 {
		return ""
	}
	contentStart := startIdx + tagEndIdx + 1

	endIdx := strings.Index(svelteContent[contentStart:], "</style>")
	if endIdx == -1 {
		return ""
	}

	return svelteContent[contentStart : contentStart+endIdx]
}

// extractTemplateSection extracts content outside of <script> and <style> tags (the template).
func extractTemplateSection(svelteContent string) string {
	result := svelteContent

	// Remove script sections
	for {
		startIdx := strings.Index(result, "<script")
		if startIdx == -1 {
			break
		}
		endIdx := strings.Index(result[startIdx:], "</script>")
		if endIdx == -1 {
			break
		}
		result = result[:startIdx] + result[startIdx+endIdx+9:]
	}

	// Remove style sections
	for {
		startIdx := strings.Index(result, "<style")
		if startIdx == -1 {
			break
		}
		endIdx := strings.Index(result[startIdx:], "</style>")
		if endIdx == -1 {
			break
		}
		result = result[:startIdx] + result[startIdx+endIdx+8:]
	}

	return result
}

// findVarDefinitions extracts CSS variable definitions from CSS content.
func findVarDefinitions(cssContent string) []string {
	var vars []string
	for _, match := range cssVarDefRegex.FindAllStringSubmatch(cssContent, -1) {
		vars = append(vars, match[1])
	}
	return vars
}

// findVarUsages extracts CSS variable usages from any content.
func findVarUsages(content string) []string {
	var vars []string
	for _, match := range cssVarUseRegex.FindAllStringSubmatch(content, -1) {
		vars = append(vars, match[1])
	}
	return vars
}

// stripCssComments removes /* ... */ comments from CSS content.
func stripCssComments(cssContent string) string {
	commentRegex := regexp.MustCompile(`/\*[\s\S]*?\*/`)
	return commentRegex.ReplaceAllString(cssContent, "")
}

// findClassDefinitions extracts CSS class definitions from CSS content.
func findClassDefinitions(cssContent string) []string {
	// Strip comments first to avoid false positives from URLs and file paths
	cleanContent := stripCssComments(cssContent)

	var classes []string
	for _, match := range cssClassDefRegex.FindAllStringSubmatch(cleanContent, -1) {
		className := match[1]
		if !reservedCssNames[className] {
			classes = append(classes, className)
		}
	}
	return classes
}

// findClassUsagesInTemplate extracts class usages from Svelte template content only.
// This should be called with template content (outside <script> and <style> sections).
func findClassUsagesInTemplate(templateContent string) []string {
	classSet := make(map[string]bool)

	// Find static class usages: class="foo bar baz"
	staticClassRegex := regexp.MustCompile(`class\s*=\s*"([^"]+)"`)
	for _, match := range staticClassRegex.FindAllStringSubmatch(templateContent, -1) {
		for _, cls := range strings.Fields(match[1]) {
			if isValidClassName(cls) {
				classSet[cls] = true
			}
		}
	}

	// Find Svelte conditional class directives: class:foo={...} or class:foo
	// This is a real class usage - when condition is true, the class is applied
	for _, match := range classDynamicRegex.FindAllStringSubmatch(templateContent, -1) {
		cls := match[1]
		if isValidClassName(cls) {
			classSet[cls] = true
		}
	}

	var classes []string
	for cls := range classSet {
		classes = append(classes, cls)
	}
	return classes
}

// isValidClassName checks if a string is a valid CSS class name (not a JS operator or event name).
func isValidClassName(s string) bool {
	if s == "" {
		return false
	}

	// Must start with letter or underscore
	first := s[0]
	if !((first >= 'a' && first <= 'z') || (first >= 'A' && first <= 'Z') || first == '_') {
		return false
	}

	// Must not contain JS operators or special chars
	invalidChars := []string{"=", "&", "|", "!", "?", "(", ")", "{", "}", "[", "]", ".", ",", ";"}
	for _, char := range invalidChars {
		if strings.Contains(s, char) {
			return false
		}
	}

	// Filter out obvious non-class patterns (event names, test IDs)
	if looksLikeEventName(s) || looksLikeTestId(s) {
		return false
	}

	return true
}

// looksLikeEventName returns true if the string looks like a Tauri/DOM event name.
func looksLikeEventName(s string) bool {
	eventPatterns := []string{
		"-complete", "-progress", "-error", "-cancelled", "-changed",
		"-mounted", "-unmounted", "-found", "-lost", "-resolved",
		"-conflict", "-state-changed",
	}
	for _, pattern := range eventPatterns {
		if strings.HasSuffix(s, pattern) {
			return true
		}
	}
	return false
}

// looksLikeTestId returns true if the string looks like a test identifier.
func looksLikeTestId(s string) bool {
	testPatterns := []string{"test-", "mock-", "invalid-", "valid-", "tampered-"}
	for _, pattern := range testPatterns {
		if strings.HasPrefix(s, pattern) {
			return true
		}
	}
	// Also filter listing-N patterns used in tests
	if strings.HasPrefix(s, "listing-") {
		return true
	}
	return false
}
