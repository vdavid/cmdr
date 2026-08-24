package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeEventCatalogFile writes content at dir/relPath, creating parent directories.
func writeEventCatalogFile(t *testing.T, dir, relPath, content string) {
	t.Helper()
	full := filepath.Join(dir, relPath)
	if err := os.MkdirAll(filepath.Dir(full), 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(full, []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
}

// catalog builds a DETAILS.md whose event section documents the given bullets.
func writeCatalog(t *testing.T, dir string, bullets ...string) {
	t.Helper()
	body := "# Analytics\n\n## Wiring\n\nNot the list.\n\n" + analyticsCatalogHeading + " (PII-free)\n\n"
	for _, b := range bullets {
		body += "- " + b + "\n"
	}
	body += "\n## After\n\nProse.\n"
	writeEventCatalogFile(t, dir, analyticsCatalogDoc, body)
}

func TestRunAnalyticsEventCatalog_EverySentEventIsDocumented(t *testing.T) {
	tmp := t.TempDir()
	writeEventCatalogFile(t, tmp, "apps/desktop/src-tauri/src/lib.rs",
		`analytics::posthog::capture("app_launched", json!({}));`)
	writeEventCatalogFile(t, tmp, "apps/desktop/src/lib/pane/loader.ts",
		`void trackEvent('pane_navigated', { volume_kind: kind })`)
	writeEventCatalogFile(t, tmp, "crates/cmdr-smb/src/volume/mod.rs",
		`vol.inner.host.analytics().record("smb_connected", &[]);`)
	writeCatalog(t,
		tmp,
		"`app_launched` (backend, `lib.rs`): no props.",
		"`pane_navigated` (frontend): `volume_kind`.",
		"`smb_connected` (backend): no host props.",
	)

	result, err := RunAnalyticsEventCatalog(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
	if !strings.Contains(result.Message, "3 PostHog events") {
		t.Errorf("expected all three events counted, got: %s", result.Message)
	}
}

func TestRunAnalyticsEventCatalog_UndocumentedEventFails(t *testing.T) {
	tmp := t.TempDir()
	writeEventCatalogFile(t, tmp, "apps/desktop/src-tauri/src/agent/wake/runner.rs",
		"crate::analytics::posthog::capture(\n    \"agent_wake\",\n    json!({}),\n);")
	writeCatalog(t, tmp, "`app_launched` (backend): no props.")
	writeEventCatalogFile(t, tmp, "apps/desktop/src-tauri/src/lib.rs",
		`analytics::posthog::capture("app_launched", json!({}));`)

	_, err := RunAnalyticsEventCatalog(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected an error for an event that no catalog bullet documents")
	}
	if !strings.Contains(err.Error(), "agent_wake") {
		t.Errorf("expected the undocumented event named, got: %v", err)
	}
	if !strings.Contains(err.Error(), "runner.rs") {
		t.Errorf("expected the emitting file named, got: %v", err)
	}
}

func TestRunAnalyticsEventCatalog_DocumentedEventWithNoEmitterFails(t *testing.T) {
	// The other drift direction: a bullet outliving the code that sent it leaves a
	// metric someone will keep waiting on.
	tmp := t.TempDir()
	writeEventCatalogFile(t, tmp, "apps/desktop/src-tauri/src/lib.rs",
		`analytics::posthog::capture("app_launched", json!({}));`)
	writeCatalog(t,
		tmp,
		"`app_launched` (backend): no props.",
		"`ghost_event` (backend): nothing sends this.",
	)

	_, err := RunAnalyticsEventCatalog(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected an error for a documented event with no emitter")
	}
	if !strings.Contains(err.Error(), "ghost_event") {
		t.Errorf("expected the orphaned event named, got: %v", err)
	}
}

func TestRunAnalyticsEventCatalog_TestFilesDontCount(t *testing.T) {
	// A fake event name in a test never ships, so it must not demand a bullet.
	tmp := t.TempDir()
	writeEventCatalogFile(t, tmp, "apps/desktop/src-tauri/src/lib.rs",
		`analytics::posthog::capture("app_launched", json!({}));`)
	writeEventCatalogFile(t, tmp, "crates/cmdr-fs/src/volume/host/host_test.rs",
		`host.analytics().record("something_happened", &[]);`)
	writeEventCatalogFile(t, tmp, "apps/desktop/src-tauri/src/agent/wake/tests/inbox.rs",
		`crate::analytics::posthog::capture("staged_in_a_test", json!({}));`)
	writeEventCatalogFile(t, tmp, "apps/desktop/src/lib/search/search.test.ts",
		`void trackEvent('search_used_in_a_test', {})`)
	writeCatalog(t, tmp, "`app_launched` (backend): no props.")

	result, err := RunAnalyticsEventCatalog(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("test files must not count as emitters: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
}

func TestRunAnalyticsEventCatalog_ReadsSlashJoinedBullets(t *testing.T) {
	// One bullet documents a family; all of its names count.
	tmp := t.TempDir()
	writeEventCatalogFile(t, tmp, "apps/desktop/src/lib/search/tracking.ts",
		"void trackEvent('search_cta_offered', { cta })\nvoid trackEvent('search_cta_used', { cta })")
	writeCatalog(t, tmp, "`search_cta_offered` / `search_cta_used` (frontend, same file): `cta` enum.")

	result, err := RunAnalyticsEventCatalog(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("both names on one bullet must count: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
}

func TestRunAnalyticsEventCatalog_BareCaptureOnlyCountsInAnalyticsRs(t *testing.T) {
	// An area's event wrappers live in `analytics.rs`, so a bare `capture("…")` there
	// is an event. The same call elsewhere is some other function, not an emitter.
	tmp := t.TempDir()
	writeEventCatalogFile(t, tmp, "apps/desktop/src-tauri/src/agent/suggested_ops/analytics.rs",
		`capture("suggestion_group_proposed", verb, op_count);`)
	writeEventCatalogFile(t, tmp, "apps/desktop/src-tauri/src/screenshots/grabber.rs",
		`let frame = capture("main_window", &opts)?;`)
	writeCatalog(t, tmp, "`suggestion_group_proposed` (backend): `verb` + `op_count`.")

	result, err := RunAnalyticsEventCatalog(&CheckContext{RootDir: tmp})
	if err != nil {
		t.Fatalf("a bare capture outside analytics.rs must not count: %v", err)
	}
	if result.Code != ResultSuccess {
		t.Errorf("expected success, got code %d: %s", result.Code, result.Message)
	}
}

func TestRunAnalyticsEventCatalog_RenamedSectionIsLoud(t *testing.T) {
	// The catalog is parsed by heading. A rename that silently emptied the list would
	// turn the check into a rubber stamp, so an empty section is an error.
	tmp := t.TempDir()
	writeEventCatalogFile(t, tmp, "apps/desktop/src-tauri/src/lib.rs",
		`analytics::posthog::capture("app_launched", json!({}));`)
	writeEventCatalogFile(t, tmp, analyticsCatalogDoc,
		"# Analytics\n\n## The events we send\n\n- `app_launched` (backend): no props.\n")

	_, err := RunAnalyticsEventCatalog(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected an error when the catalog section holds no event bullets")
	}
	if !strings.Contains(err.Error(), "renamed") {
		t.Errorf("expected the error to point at the heading, got: %v", err)
	}
}
