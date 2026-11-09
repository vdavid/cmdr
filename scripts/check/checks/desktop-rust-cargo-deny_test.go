package checks

import (
	"encoding/json"
	"strings"
	"testing"
	"time"
)

// cargoDenyFixture is trimmed from a real `cargo deny --format json check` run (cargo-deny
// 0.18.9, 2026-10-14): two rustls advisories, a duplicate of one (cargo-deny repeats an advisory
// per affected crate version), a license rejection, a warning-level diagnostic, and the summary.
const cargoDenyFixture = `    Fetching advisory database from 'https://github.com/RustSec/advisory-db'
{"fields":{"advisory":{"id":"RUSTSEC-2026-0346","package":"rustls","title":"` + "`Debug`" + ` impls for client session values expose TLS secrets"},"code":"vulnerability","graphs":[{"Krate":{"name":"rustls","version":"0.23.45"}}],"message":"` + "`Debug`" + ` impls for client session values expose TLS secrets","severity":"error"},"type":"diagnostic"}
{"fields":{"advisory":{"id":"RUSTSEC-2026-0349","package":"rustls","title":"Unauthenticated ServerHello can downgrade later connections to classical key exchange"},"code":"vulnerability","graphs":[{"Krate":{"name":"rustls","version":"0.23.45"}}],"message":"Unauthenticated ServerHello can downgrade later connections to classical key exchange","severity":"error"},"type":"diagnostic"}
{"fields":{"advisory":{"id":"RUSTSEC-2026-0349","package":"rustls","title":"Unauthenticated ServerHello can downgrade later connections to classical key exchange"},"code":"vulnerability","graphs":[{"Krate":{"name":"rustls","version":"0.23.45"}}],"message":"Unauthenticated ServerHello can downgrade later connections to classical key exchange","severity":"error"},"type":"diagnostic"}
{"fields":{"code":"rejected","graphs":[{"Krate":{"name":"leftpad","version":"1.0.0"}}],"message":"failed to satisfy license requirements","severity":"error"},"type":"diagnostic"}
{"fields":{"advisory":{"id":"RUSTSEC-2025-0052","package":"async-std","title":"async-std has been discontinued"},"code":"advisory-ignored","graphs":[],"message":"advisory was ignored","severity":"note"},"type":"diagnostic"}
{"fields":{"advisories":{"errors":3,"helps":0,"notes":1,"warnings":0},"bans":{"errors":0,"helps":0,"notes":0,"warnings":0},"licenses":{"errors":1,"helps":0,"notes":0,"warnings":0},"sources":{"errors":0,"helps":0,"notes":0,"warnings":0}},"type":"summary"}
`

func TestParseCargoDenyDiagnostics(t *testing.T) {
	advisories, others := parseCargoDenyDiagnostics(cargoDenyFixture)

	want := []vulnAdvisory{
		{ID: "RUSTSEC-2026-0346", Summary: "`Debug` impls for client session values expose TLS secrets",
			Ecosystem: "crates.io", Module: "rustls", Version: "0.23.45"},
		{ID: "RUSTSEC-2026-0349", Summary: "Unauthenticated ServerHello can downgrade later connections to classical key exchange",
			Ecosystem: "crates.io", Module: "rustls", Version: "0.23.45"},
	}
	if len(advisories) != len(want) {
		t.Fatalf("want one advisory per ID and crate version, got %d: %+v", len(advisories), advisories)
	}
	for i := range want {
		if advisories[i] != want[i] {
			t.Errorf("advisory %d =\n  %+v\nwant\n  %+v", i, advisories[i], want[i])
		}
	}
	if len(others) != 1 || !strings.Contains(others[0], "leftpad") {
		t.Errorf("want the license rejection as the one non-advisory error, got %q", others)
	}
}

// The rustls case from getcmdr/cmdr#433: 0.23.46 fixed four advisories on 2026-10-14 09:54 UTC,
// so it's adoptable from 2026-10-17 09:54 and fails from 2026-10-20 09:54.
func TestCargoDenyAdvisoriesFollowTheFixAgePolicy(t *testing.T) {
	released := time.Date(2026, 10, 14, 9, 54, 0, 0, time.UTC)
	advisories, _ := parseCargoDenyDiagnostics(cargoDenyFixture)
	withFix := func(a vulnAdvisory) vulnAdvisory {
		if a.ID == "RUSTSEC-2026-0346" || a.ID == "RUSTSEC-2026-0349" {
			a.FixedVersion = "0.23.46"
		}
		return a
	}
	fixReleased := func(a vulnAdvisory) (time.Time, bool) { return released, a.FixedVersion == "0.23.46" }

	blocking, waiting := splitCargoDenyAdvisories(advisories, time.Date(2026, 10, 14, 18, 0, 0, 0, time.UTC), withFix, fixReleased)
	if len(blocking) != 0 || len(waiting) != 2 {
		t.Fatalf("on release day both should wait, got blocking %+v, waiting %+v", blocking, waiting)
	}
	if want := time.Date(2026, 10, 17, 9, 54, 0, 0, time.UTC); !waiting[0].FixAdoptable.Equal(want) {
		t.Errorf("FixAdoptable = %s, want %s", waiting[0].FixAdoptable, want)
	}
	if want := time.Date(2026, 10, 20, 9, 54, 0, 0, time.UTC); !waiting[0].FixDue.Equal(want) {
		t.Errorf("FixDue = %s, want %s", waiting[0].FixDue, want)
	}

	blocking, waiting = splitCargoDenyAdvisories(advisories, time.Date(2026, 10, 20, 9, 54, 0, 0, time.UTC), withFix, fixReleased)
	if len(blocking) != 2 || len(waiting) != 0 {
		t.Errorf("once the grace ends both should fail, got blocking %+v, waiting %+v", blocking, waiting)
	}

	noFix := func(a vulnAdvisory) vulnAdvisory { return a }
	blocking, _ = splitCargoDenyAdvisories(advisories, time.Date(2026, 10, 14, 18, 0, 0, 0, time.UTC), noFix, fixReleased)
	if len(blocking) != 2 {
		t.Errorf("an advisory with no known fix fails at once, got %+v", blocking)
	}
}

// osvRustlsRecord is trimmed from `https://api.osv.dev/v1/vulns/RUSTSEC-2026-0346` (2026-10-14).
const osvRustlsRecord = `{"id":"RUSTSEC-2026-0346","published":"2026-10-14T12:00:00Z","summary":"Debug impls expose TLS secrets",
  "affected":[{"package":{"name":"rustls","ecosystem":"crates.io"},"ranges":[{"type":"SEMVER","events":[{"introduced":"0.21.0"},{"fixed":"0.23.46"}]}]}]}`

// osvFixFor finds the fix an OSV record names for the version we use, so cargo-deny and
// osv-scanner agree on which upgrade clears an advisory.
func TestOSVFixFor(t *testing.T) {
	var v osvVulnerability
	if err := json.Unmarshal([]byte(osvRustlsRecord), &v); err != nil {
		t.Fatal(err)
	}

	a := vulnAdvisory{ID: "RUSTSEC-2026-0346", Summary: "kept", Ecosystem: "crates.io", Module: "rustls", Version: "0.23.45"}
	got := osvFixFor(a, v)
	if got.FixedVersion != "0.23.46" || got.Summary != "kept" || !got.Published.Equal(v.Published) {
		t.Errorf("osvFixFor = %+v, want fix 0.23.46, the original summary, and the OSV publish date", got)
	}
}
