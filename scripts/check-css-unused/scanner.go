package main

import (
	"os"
	"path/filepath"
)

// ScanResult holds all definitions and usages found during scanning.
type ScanResult struct {
	VarDefs      map[string][]string // var name -> files where defined
	VarUses      map[string]bool     // var names that are used
	ClassDefs    map[string][]string // class name -> files where defined
	ClassUses    map[string]bool     // class names that are used
	ClassUseLocs map[string][]string // class name -> files where used (for undefined detection)
}

// NewScanResult creates an initialized ScanResult.
func NewScanResult() *ScanResult {
	return &ScanResult{
		VarDefs:      make(map[string][]string),
		VarUses:      make(map[string]bool),
		ClassDefs:    make(map[string][]string),
		ClassUses:    make(map[string]bool),
		ClassUseLocs: make(map[string][]string),
	}
}

// ScanDesktopApp scans the desktop app source directory for CSS definitions and usages.
func ScanDesktopApp(srcDir string) (*ScanResult, error) {
	result := NewScanResult()

	// Process app.css first (global styles)
	appCssPath := filepath.Join(srcDir, "app.css")
	if err := processAppCss(appCssPath, result); err != nil {
		return nil, err
	}

	// Walk all source files
	err := filepath.Walk(srcDir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		if info.IsDir() {
			return nil
		}
		if path == appCssPath {
			return nil // Already processed
		}

		ext := filepath.Ext(path)
		relPath, _ := filepath.Rel(srcDir, path)

		switch ext {
		case ".svelte":
			return processSvelteFile(path, relPath, result)
		case ".ts":
			return processTsFile(path, result)
		case ".css":
			return processCssFile(path, relPath, result)
		}
		return nil
	})

	return result, err
}

func processAppCss(path string, result *ScanResult) error {
	content, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	text := string(content)

	// Find variable definitions
	for _, varName := range findVarDefinitions(text) {
		result.VarDefs[varName] = append(result.VarDefs[varName], "app.css")
	}

	// Find variable usages (vars can reference other vars)
	for _, varName := range findVarUsages(text) {
		result.VarUses[varName] = true
	}

	// Find class definitions in app.css (global classes)
	for _, className := range findClassDefinitions(text) {
		result.ClassDefs[className] = append(result.ClassDefs[className], "app.css")
	}

	return nil
}

func processSvelteFile(path, relPath string, result *ScanResult) error {
	content, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	text := string(content)

	// Extract and process style section for definitions
	styleContent := extractStyleSection(text)
	for _, className := range findClassDefinitions(styleContent) {
		result.ClassDefs[className] = append(result.ClassDefs[className], relPath)
	}
	for _, varName := range findVarDefinitions(styleContent) {
		result.VarDefs[varName] = append(result.VarDefs[varName], relPath)
	}

	// Find variable usages anywhere (including style section for var references)
	for _, varName := range findVarUsages(text) {
		result.VarUses[varName] = true
	}

	// Find class usages only in template section (not script, to avoid false positives)
	templateContent := extractTemplateSection(text)
	for _, className := range findClassUsagesInTemplate(templateContent) {
		result.ClassUses[className] = true
		result.ClassUseLocs[className] = append(result.ClassUseLocs[className], relPath)
	}

	return nil
}

func processTsFile(path string, result *ScanResult) error {
	content, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	text := string(content)

	// Only look for CSS variable usages in TS files
	// Don't try to detect class usages - too many false positives from strings
	for _, varName := range findVarUsages(text) {
		result.VarUses[varName] = true
	}

	return nil
}

func processCssFile(path, relPath string, result *ScanResult) error {
	content, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	text := string(content)

	for _, className := range findClassDefinitions(text) {
		result.ClassDefs[className] = append(result.ClassDefs[className], relPath)
	}
	for _, varName := range findVarDefinitions(text) {
		result.VarDefs[varName] = append(result.VarDefs[varName], relPath)
	}
	for _, varName := range findVarUsages(text) {
		result.VarUses[varName] = true
	}

	return nil
}
