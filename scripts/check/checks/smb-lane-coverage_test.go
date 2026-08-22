package checks

import (
	"strings"
	"testing"
)

// The lane's filter and the name this check enforces are one contract. If the
// filter is ever re-keyed, this fails rather than letting the check go on
// enforcing a prefix nothing selects on.
func TestSmbLaneFilterCarriesTheEnforcedPrefix(t *testing.T) {
	if !strings.Contains(smbIntegrationFilter, smbLanePrefix) {
		t.Fatalf("the integration lane filters on %q, which doesn't carry %q", smbIntegrationFilter, smbLanePrefix)
	}
}

func TestFindOutOfLaneSmbCells(t *testing.T) {
	tests := []struct {
		name  string
		files map[string]string
		want  []string // "file:line fn"
	}{
		{
			name: "a gated cell carrying the prefix is in the lane",
			files: map[string]string{"backends/smb_stress_test.rs": `
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_copy_100_unique_files() {}
`},
		},
		{
			// The failure this check exists for: the cell needs the fixture, the
			// lane won't select it, and everything stays green while it never runs.
			name: "a gated cell without the prefix is a finding",
			files: map[string]string{"volume/smb_scan_oracle_tests.rs": `
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_scan_uses_oracle_on_hit_skips_stat_pipeline() {}
`},
			want: []string{"volume/smb_scan_oracle_tests.rs:4 smb_scan_uses_oracle_on_hit_skips_stat_pipeline"},
		},
		{
			name: "a compose service name gates just as well as the start script",
			files: map[string]string{"volume/maxread.rs": `
#[tokio::test]
#[ignore = "Requires docker-compose smb-consumer-maxreadsize on port 10494 (started by start.sh core)"]
async fn reads_at_the_negotiated_maximum() {}
`},
			want: []string{"volume/maxread.rs:4 reads_at_the_negotiated_maximum"},
		},
		{
			name: "a cell deliberately out of the lane says so and is left alone",
			files: map[string]string{"transfer/copy_concurrency_bench.rs": `
#[tokio::test]
// allowed-out-of-lane-smb-cell: a measurement harness, not an assertion; run on demand
#[ignore = "Measurement harness: needs Docker SMB (./apps/desktop/test/smb-servers/start.sh) or a reachable NAS"]
async fn concurrency_bench_sweep_window_against_wall_clock() {}
`},
		},
		{
			name: "an ignored test gated on something else is none of this check's business",
			files: map[string]string{"ai/client_real_openai_test.rs": `
#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn chat_completion_streams() {}
`},
			want: nil,
		},
		{
			// nextest matches the prefix anywhere in the test path, module segments
			// included, so a cell in a module that carries it is selected too.
			name: "a cell whose own module carries the prefix is in the lane",
			files: map[string]string{"volume/smb_integration_extras.rs": `
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn reads_a_file_over_the_session() {}
`},
		},
		{
			name: "attributes between the gate and the test don't hide it",
			files: map[string]string{"volume/timed.rs": `
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
#[allow(clippy::too_many_lines)]
async fn reads_a_file_over_the_session() {}
`},
			want: []string{"volume/timed.rs:5 reads_a_file_over_the_session"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var got []string
			for _, cell := range scanSmbLaneCoverage(tt.files).stranded {
				got = append(got, cell.String())
			}
			if strings.Join(got, " | ") != strings.Join(tt.want, " | ") {
				t.Errorf("findOutOfLaneSmbCells = %v, want %v", got, tt.want)
			}
		})
	}
}

// The opt-out has to carry a reason, or it becomes a way to silence the check
// without saying anything.
func TestOutOfLaneOptOutNeedsAReason(t *testing.T) {
	files := map[string]string{"volume/bare.rs": `
#[tokio::test]
// allowed-out-of-lane-smb-cell
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn reads_a_file_over_the_session() {}
`}
	if got := scanSmbLaneCoverage(files).stranded; len(got) != 1 {
		t.Errorf("a reasonless opt-out should not exempt anything; got %v", got)
	}
}

// An opt-out left behind after its cell was renamed into the lane excuses
// nothing, and a silent one is how the next real opt-out gets waved through.
func TestOrphanedOutOfLaneOptOutIsReported(t *testing.T) {
	files := map[string]string{"volume/renamed.rs": `
#[tokio::test]
// allowed-out-of-lane-smb-cell: it used to be a bench
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_reads_a_file_over_the_session() {}
`}
	scan := scanSmbLaneCoverage(files)
	if len(scan.stranded) != 0 {
		t.Errorf("the cell is in the lane; got %v", scan.stranded)
	}
	if len(scan.orphans) != 1 {
		t.Fatalf("orphans = %v, want the stale opt-out reported", scan.orphans)
	}
	if scan.orphans[0].line != 3 {
		t.Errorf("orphan line = %d, want 3", scan.orphans[0].line)
	}
}
