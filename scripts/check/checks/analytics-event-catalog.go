package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// The catalog every PostHog event name has to appear in, and the section of it
// that holds the list.
const (
	analyticsCatalogDoc     = "apps/desktop/src-tauri/src/analytics/DETAILS.md"
	analyticsCatalogHeading = "## Starter event set"
)

// Source roots the emitter scan walks. The website and the analytics dashboard
// send their own PostHog events through PostHog's own SDK (`$pageview` and
// friends), which this catalog doesn't cover.
var analyticsEventRoots = []string{
	"apps/desktop/src",
	"apps/desktop/src-tauri/src",
	"crates",
}

// Backend emitters. `posthog::capture("name", …)` is the direct form; the bare
// `capture("name", …)` form is accepted only inside a file named `analytics.rs`,
// which is where an area's event wrappers live by convention (a bare `capture(`
// elsewhere is some other function).
var (
	posthogCaptureRe = regexp.MustCompile(`posthog::capture\(\s*"([a-z0-9_]+)"`)
	bareCaptureRe    = regexp.MustCompile(`\bcapture\(\s*"([a-z0-9_]+)"`)
	// The `AnalyticsSink` seam the backend crates use, since they can't see `tauri`.
	analyticsSinkRe = regexp.MustCompile(`analytics\(\)\.record\(\s*"([a-z0-9_]+)"`)
	// The frontend's one path, the `track_event` IPC wrapper.
	trackEventRe = regexp.MustCompile(`trackEvent\(\s*['"]([a-z0-9_]+)['"]`)
	// A catalog bullet opens with one or more backticked names joined by " / ".
	catalogNameRe = regexp.MustCompile("^`([a-z0-9_]+)`")
	catalogJoinRe = regexp.MustCompile("^ / `([a-z0-9_]+)`")
)

// analyticsEmitter is one event name and the repo-relative file that sends it.
type analyticsEmitter struct {
	event string
	file  string
}

// RunAnalyticsEventCatalog pins the PostHog event vocabulary to its catalog: every
// event name emitted in the tree must be documented in the analytics DETAILS.md,
// and every documented name must have a live emitter.
//
// Why it's worth a check. An event is written once and then read months later off a
// dashboard, so nothing about the code says whether a metric reading zero is a
// feature nobody uses or an emitter that never shipped. The catalog is where that
// question gets answered, which only works if the catalog is complete: three events
// were firing undocumented before this check existed, including the three carrying
// the agent's north-star acceptance rate.
//
// It's an error, not a warning: both directions are cheap to fix (one bullet, or one
// deleted bullet) and a drifting catalog is worth nothing.
//
// Coverage is honest rather than total: it recognizes the shapes documented in
// `analytics/DETAILS.md` § How to add an event. An event smuggled in through a
// helper that takes a runtime `&str` is invisible to it, and the answer to that is
// not to write one.
func RunAnalyticsEventCatalog(ctx *CheckContext) (CheckResult, error) {
	emitters, err := findAnalyticsEmitters(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	documented, err := readAnalyticsCatalog(filepath.Join(ctx.RootDir, analyticsCatalogDoc))
	if err != nil {
		return CheckResult{}, err
	}

	emitted := make(map[string][]string)
	for _, e := range emitters {
		emitted[e.event] = append(emitted[e.event], e.file)
	}

	var undocumented, orphaned []string
	for event, files := range emitted {
		if !documented[event] {
			sort.Strings(files)
			undocumented = append(undocumented, fmt.Sprintf("%s (sent from %s)", event, strings.Join(dedupe(files), ", ")))
		}
	}
	for event := range documented {
		if _, ok := emitted[event]; !ok {
			orphaned = append(orphaned, event)
		}
	}

	if len(undocumented) == 0 && len(orphaned) == 0 {
		return Success(fmt.Sprintf("%d PostHog %s documented in the catalog",
			len(emitted), Pluralize(len(emitted), "event", "events"))), nil
	}
	return CheckResult{}, fmt.Errorf("%s", formatAnalyticsCatalogDrift(undocumented, orphaned))
}

// findAnalyticsEmitters walks the source roots and collects every event name sent
// through one of the four emitter shapes. Test files are skipped: a fake event name
// in a test is not a shipped event.
func findAnalyticsEmitters(rootDir string) ([]analyticsEmitter, error) {
	var found []analyticsEmitter
	for _, root := range analyticsEventRoots {
		abs := filepath.Join(rootDir, root)
		if !fileExists(abs) {
			continue
		}
		err := filepath.WalkDir(abs, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() {
				if d.Name() == "tests" || d.Name() == "node_modules" || d.Name() == "target" {
					return filepath.SkipDir
				}
				return nil
			}
			if !isAnalyticsScannableFile(path) {
				return nil
			}
			data, readErr := os.ReadFile(path)
			if readErr != nil {
				return readErr
			}
			rel, relErr := filepath.Rel(rootDir, path)
			if relErr != nil {
				return relErr
			}
			for _, event := range analyticsEventsIn(string(data), filepath.Base(path)) {
				found = append(found, analyticsEmitter{event: event, file: rel})
			}
			return nil
		})
		if err != nil {
			return nil, fmt.Errorf("failed to scan %s for analytics events: %w", root, err)
		}
	}
	return found, nil
}

// analyticsEventsIn returns the event names one source file sends. `base` is the
// file's name, which decides whether the bare `capture("…")` form counts.
func analyticsEventsIn(source, base string) []string {
	patterns := []*regexp.Regexp{posthogCaptureRe, analyticsSinkRe, trackEventRe}
	if base == "analytics.rs" {
		patterns = append(patterns, bareCaptureRe)
	}
	var events []string
	for _, re := range patterns {
		for _, m := range re.FindAllStringSubmatch(source, -1) {
			events = append(events, m[1])
		}
	}
	return dedupe(events)
}

// isAnalyticsScannableFile reports whether a file is first-party source that could
// send an event. Tests are excluded, since a test's event name never ships.
func isAnalyticsScannableFile(path string) bool {
	base := filepath.Base(path)
	switch {
	case strings.HasSuffix(base, "_test.rs"), strings.HasSuffix(base, "_tests.rs"):
		return false
	case strings.HasSuffix(base, ".test.ts"), strings.HasSuffix(base, ".spec.ts"),
		strings.HasSuffix(base, ".test.svelte.ts"), strings.HasSuffix(base, ".svelte.test.ts"):
		return false
	}
	switch filepath.Ext(base) {
	case ".rs", ".ts", ".svelte":
		return true
	}
	return false
}

// readAnalyticsCatalog parses the documented event names out of the catalog
// section. A bullet opens with the names it documents, backticked and joined by
// " / " (`search_cta_offered` / `search_cta_used`), so the names are read off the
// front of the bullet and everything after the first non-name token is prose.
func readAnalyticsCatalog(path string) (map[string]bool, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("failed to read the analytics event catalog at %s: %w", analyticsCatalogDoc, err)
	}
	documented := make(map[string]bool)
	inSection := false
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "## ") {
			inSection = strings.HasPrefix(line, analyticsCatalogHeading)
			continue
		}
		if !inSection || !strings.HasPrefix(line, "- ") {
			continue
		}
		rest := strings.TrimPrefix(line, "- ")
		m := catalogNameRe.FindStringSubmatch(rest)
		for m != nil {
			documented[m[1]] = true
			rest = rest[len(m[0]):]
			m = catalogJoinRe.FindStringSubmatch(rest)
		}
	}
	if len(documented) == 0 {
		return nil, fmt.Errorf("found no event bullets under %q in %s: has the section been renamed?",
			analyticsCatalogHeading, analyticsCatalogDoc)
	}
	return documented, nil
}

// formatAnalyticsCatalogDrift builds the failure body for both drift directions.
func formatAnalyticsCatalogDrift(undocumented, orphaned []string) string {
	sort.Strings(undocumented)
	sort.Strings(orphaned)

	var sb strings.Builder
	if len(undocumented) > 0 {
		sb.WriteString(fmt.Sprintf("%d event %s sent but not documented:\n",
			len(undocumented), Pluralize(len(undocumented), "is", "are")))
		for _, u := range undocumented {
			sb.WriteString("  - " + u + "\n")
		}
	}
	if len(orphaned) > 0 {
		sb.WriteString(fmt.Sprintf("%d documented event %s no emitter:\n",
			len(orphaned), Pluralize(len(orphaned), "has", "have")))
		for _, o := range orphaned {
			sb.WriteString("  - " + o + "\n")
		}
	}
	sb.WriteString(fmt.Sprintf(
		"Add or remove the bullet in %s § %q. The catalog is how anyone reading a dashboard "+
			"months from now tells an unused feature from an emitter that never shipped, so it only "+
			"works while it's complete.",
		analyticsCatalogDoc, analyticsCatalogHeading))
	return sb.String()
}

// dedupe returns the input with duplicates removed, order preserved.
func dedupe(values []string) []string {
	seen := make(map[string]bool, len(values))
	out := make([]string, 0, len(values))
	for _, v := range values {
		if !seen[v] {
			seen[v] = true
			out = append(out, v)
		}
	}
	return out
}
