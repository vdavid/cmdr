package main

import (
	"slices"
	"testing"
)

func TestFindClassDefinitionsIgnoresImportFilenames(t *testing.T) {
	// An `@import` filename ends in `.css`, which the class-def regex must not
	// pick up as a `.css` class (regression: splitting app.css into @imported
	// partials introduced the first @import and surfaced this false positive).
	css := `@import './app-palette.css';
@import url("app-reset.css");
.real-class { color: red; }`

	got := findClassDefinitions(css)

	if slices.Contains(got, "css") {
		t.Errorf("findClassDefinitions picked up a `.css` class from an @import filename: %v", got)
	}
	if !slices.Contains(got, "real-class") {
		t.Errorf("findClassDefinitions dropped a real class; got %v", got)
	}
}

func TestStripCssImports(t *testing.T) {
	in := `@import './app-palette.css';
.keep { color: red; }
@import url("x.css");`
	out := stripCssImports(in)
	if want := "\n.keep { color: red; }\n"; out != want {
		t.Errorf("stripCssImports = %q, want %q", out, want)
	}
}
