package checks

import (
	"strings"
	"testing"
)

// A fake nextest invocation: maps profile -> the output that run produces.
type fakeRun struct {
	calls   []fakeCall
	outputs map[string]string
}

type fakeCall struct {
	profile string
	names   []string
}

func (f *fakeRun) run(profile string, names []string) (string, error) {
	f.calls = append(f.calls, fakeCall{profile: profile, names: names})
	out, ok := f.outputs[profile]
	if !ok {
		return "", nil // nothing failed
	}
	return out, nil
}

// failureOutput renders a minimal nextest summary in which every named test fails at
// the cap, so a fake re-run can report "still failing".
func failureOutput(names ...string) string {
	var b strings.Builder
	b.WriteString("────────────\n")
	for _, n := range names {
		b.WriteString("     TIMEOUT [   8.002s] (1/1) cmdr_lib " + n + "\n")
	}
	return b.String()
}

func capKill(name string) RustFailure {
	return RustFailure{Binary: "cmdr_lib", Name: name, Class: ClassNextestCap}
}

// Load samplers for the two machine states the verdicts care about.
func quiet() float64 { return 0.2 }
func busy() float64  { return 12.0 }

// Passing alone at the UNCHANGED deadline is the signal that the suite itself was
// starving the test. No escalation run should even happen.
func TestATestThatPassesAloneAtTheSameDeadlineIsContention(t *testing.T) {
	f := &fakeRun{outputs: map[string]string{}} // probe reports nothing failing
	results := ClassifyContention([]RustFailure{capKill("a::b")}, f.run, quiet)

	if len(results) != 1 {
		t.Fatalf("expected 1 result, got %+v", results)
	}
	if results[0].Verdict != VerdictContention {
		t.Errorf("verdict = %q, want %q", results[0].Verdict, VerdictContention)
	}
	if len(f.calls) != 1 {
		t.Fatalf("expected only the probe run, got %d calls: %+v", len(f.calls), f.calls)
	}
	if f.calls[0].profile != ContentionProbeProfile {
		t.Errorf("probe ran under profile %q, want %q", f.calls[0].profile, ContentionProbeProfile)
	}
}

// Failing alone at the same deadline but passing with headroom means the test got
// slower, not starved. That must NOT be absorbed as contention.
func TestATestThatNeedsHeadroomAloneIsTooSlowNotContention(t *testing.T) {
	f := &fakeRun{outputs: map[string]string{
		ContentionProbeProfile: failureOutput("a::b"),
		// escalation profile absent => nothing failed there
	}}
	results := ClassifyContention([]RustFailure{capKill("a::b")}, f.run, quiet)

	if len(results) != 1 {
		t.Fatalf("expected 1 result, got %+v", results)
	}
	if results[0].Verdict != VerdictTooSlow {
		t.Errorf("verdict = %q, want %q", results[0].Verdict, VerdictTooSlow)
	}
	if len(f.calls) != 2 {
		t.Fatalf("expected probe + escalation, got %+v", f.calls)
	}
	if f.calls[1].profile != ContentionRetryProfile {
		t.Errorf("escalation ran under %q, want %q", f.calls[1].profile, ContentionRetryProfile)
	}
}

func TestATestThatFailsEvenWithHeadroomIsARealFailure(t *testing.T) {
	f := &fakeRun{outputs: map[string]string{
		ContentionProbeProfile: failureOutput("a::b"),
		ContentionRetryProfile: failureOutput("a::b"),
	}}
	results := ClassifyContention([]RustFailure{capKill("a::b")}, f.run, quiet)

	if len(results) != 1 {
		t.Fatalf("expected 1 result, got %+v", results)
	}
	if results[0].Verdict != VerdictReal {
		t.Errorf("verdict = %q, want %q", results[0].Verdict, VerdictReal)
	}
}

// Mixed batch: only the still-failing tests may be escalated, never the ones the probe
// already cleared. Re-running a cleared test wastes a slot and can flip it by luck.
func TestOnlyStillFailingTestsAreEscalated(t *testing.T) {
	f := &fakeRun{outputs: map[string]string{
		ContentionProbeProfile: failureOutput("slow::one"),
		ContentionRetryProfile: "",
	}}
	results := ClassifyContention(
		[]RustFailure{capKill("starved::one"), capKill("slow::one")},
		f.run, quiet,
	)

	byName := map[string]ContentionResult{}
	for _, r := range results {
		byName[r.Name] = r
	}
	if byName["starved::one"].Verdict != VerdictContention {
		t.Errorf("starved::one = %q", byName["starved::one"].Verdict)
	}
	if byName["slow::one"].Verdict != VerdictTooSlow {
		t.Errorf("slow::one = %q", byName["slow::one"].Verdict)
	}

	if len(f.calls) != 2 {
		t.Fatalf("expected 2 calls, got %+v", f.calls)
	}
	if len(f.calls[1].names) != 1 || f.calls[1].names[0] != "slow::one" {
		t.Errorf("escalation should carry only the still-failing test, got %v", f.calls[1].names)
	}
}

// Leaks are a nextest PASS status. Re-running them would be nonsense, and counting them
// as failures overstates a red run.
func TestLeaksAreNotTreatedAsFailures(t *testing.T) {
	failures := []RustFailure{
		{Binary: "cmdr_lib", Name: "leaky::one", Class: ClassLeak},
		capKill("starved::one"),
	}
	real := RealFailures(failures)
	if len(real) != 1 || real[0].Name != "starved::one" {
		t.Fatalf("RealFailures should drop leaks, got %+v", real)
	}
}

func TestDiagnosisCountsOnlyRealFailures(t *testing.T) {
	failures := []RustFailure{
		{Binary: "cmdr_lib", Name: "leaky::one", Class: ClassLeak},
		capKill("starved::one"),
	}
	d := DiagnoseRustFailures(failures)
	if !strings.Contains(d, "1 failing test") {
		t.Errorf("header should count the 1 real failure, not the leak:\n%s", d)
	}
	if !strings.Contains(d, "leaky::one") {
		t.Errorf("the leak should still be reported:\n%s", d)
	}
}

// The re-run is bounded. Past the cap the honest answer is that the machine was too
// loaded to conclude anything, and the report must SAY the cap was hit rather than
// silently examining a subset.
func TestTooManyFailuresSkipsTheRerunAndDisclosesTheCap(t *testing.T) {
	var many []RustFailure
	for i := 0; i < MaxContentionRerun+1; i++ {
		many = append(many, capKill("t::"+string(rune('a'+i))))
	}
	f := &fakeRun{outputs: map[string]string{}}
	results, skipped := MaybeClassifyContention(many, f.run, quiet)

	if !skipped {
		t.Fatal("expected the re-run to be skipped past the cap")
	}
	if results != nil {
		t.Errorf("no results should be produced when skipped, got %+v", results)
	}
	if len(f.calls) != 0 {
		t.Errorf("nothing should have been run, got %+v", f.calls)
	}

	note := ContentionSkippedNote(len(many))
	if !strings.Contains(note, "15") {
		t.Errorf("the note must disclose the cap: %q", note)
	}
	if !strings.Contains(note, "16") {
		t.Errorf("the note must say how many failed: %q", note)
	}
}

func TestUnderTheCapTheRerunProceeds(t *testing.T) {
	f := &fakeRun{outputs: map[string]string{}}
	_, skipped := MaybeClassifyContention([]RustFailure{capKill("a::b")}, f.run, quiet)
	if skipped {
		t.Fatal("a single failure is well under the cap")
	}
}

// The summary is what a human or agent actually reads, so it must state the verdicts
// plainly and never imply a clean pass.
func TestContentionSummaryStatesEachVerdict(t *testing.T) {
	results := []ContentionResult{
		{Name: "starved::one", Verdict: VerdictContention},
		{Name: "slow::one", Verdict: VerdictTooSlow},
		{Name: "broken::one", Verdict: VerdictReal},
	}
	summary := ContentionSummary(results, 42.5)

	for _, want := range []string{"starved::one", "slow::one", "broken::one", "load"} {
		if !strings.Contains(summary, want) {
			t.Errorf("summary missing %q:\n%s", want, summary)
		}
	}
}

// The motivating scenario: the agents that saturated the machine are STILL running when
// the re-run happens. "Needed headroom" then can't distinguish starvation from real
// slowness, so it must not be reported as the latter.
func TestNeedingHeadroomOnABusyMachineIsInconclusiveNotTooSlow(t *testing.T) {
	f := &fakeRun{outputs: map[string]string{
		ContentionProbeProfile: failureOutput("a::b"),
	}}
	results := ClassifyContention([]RustFailure{capKill("a::b")}, f.run, busy)

	if len(results) != 1 {
		t.Fatalf("expected 1 result, got %+v", results)
	}
	if results[0].Verdict != VerdictInconclusive {
		t.Errorf("verdict = %q, want %q", results[0].Verdict, VerdictInconclusive)
	}
}

// A busy machine excuses nothing about a test that fails even with headroom.
func TestABusyMachineStillReportsARealFailure(t *testing.T) {
	f := &fakeRun{outputs: map[string]string{
		ContentionProbeProfile: failureOutput("a::b"),
		ContentionRetryProfile: failureOutput("a::b"),
	}}
	results := ClassifyContention([]RustFailure{capKill("a::b")}, f.run, busy)

	if len(results) != 1 {
		t.Fatalf("expected 1 result, got %+v", results)
	}
	if results[0].Verdict != VerdictReal {
		t.Errorf("a busy machine must not excuse a genuine failure, got %q", results[0].Verdict)
	}
}

func TestWarnOnlyToleratesContentionAndInconclusiveButNothingElse(t *testing.T) {
	cases := []struct {
		name    string
		results []ContentionResult
		want    bool
	}{
		{"all contention", []ContentionResult{{Verdict: VerdictContention}}, true},
		{"contention plus inconclusive", []ContentionResult{{Verdict: VerdictContention}, {Verdict: VerdictInconclusive}}, true},
		{"any too-slow", []ContentionResult{{Verdict: VerdictContention}, {Verdict: VerdictTooSlow}}, false},
		{"any real", []ContentionResult{{Verdict: VerdictInconclusive}, {Verdict: VerdictReal}}, false},
		{"empty", nil, false},
	}
	for _, c := range cases {
		if got := WarnOnly(c.results); got != c.want {
			t.Errorf("%s: WarnOnly = %v, want %v", c.name, got, c.want)
		}
	}
}

func TestSummaryReportsInconclusiveDistinctly(t *testing.T) {
	summary := ContentionSummary([]ContentionResult{{Name: "murky::one", Verdict: VerdictInconclusive}}, 42.5)
	if !strings.Contains(summary, "murky::one") {
		t.Errorf("summary should name the test:\n%s", summary)
	}
	if !strings.Contains(summary, "still busy") {
		t.Errorf("summary should say why it's inconclusive:\n%s", summary)
	}
}
