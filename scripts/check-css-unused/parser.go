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

// reservedCssNames contains names that should not be treated as class definitions.
// Note: We don't filter pseudo-class names here because our regex only matches
// .classname patterns, not :pseudo patterns. The only exception is :global()
// which Svelte uses for unscoped styles.
var reservedCssNames = map[string]bool{
	"global": true, // Svelte's :global() modifier, not a real class
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

	return true
}
