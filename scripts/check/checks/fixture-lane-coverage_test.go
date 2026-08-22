package checks

import (
	"strings"
	"testing"
)

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
// allowed-out-of-lane-fixture-cell: a measurement harness, not an assertion; run on demand
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
			for _, cell := range scanFixtureLaneCoverage(tt.files).stranded {
				got = append(got, cell.String())
			}
			if strings.Join(got, " | ") != strings.Join(tt.want, " | ") {
				t.Errorf("scanFixtureLaneCoverage = %v, want %v", got, tt.want)
			}
		})
	}
}

// The opt-out has to carry a reason, or it becomes a way to silence the check
// without saying anything.
func TestOutOfLaneOptOutNeedsAReason(t *testing.T) {
	files := map[string]string{"volume/bare.rs": `
#[tokio::test]
// allowed-out-of-lane-fixture-cell
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn reads_a_file_over_the_session() {}
`}
	if got := scanFixtureLaneCoverage(files).stranded; len(got) != 1 {
		t.Errorf("a reasonless opt-out should not exempt anything; got %v", got)
	}
}

// An opt-out left behind after its cell was renamed into the lane excuses
// nothing, and a silent one is how the next real opt-out gets waved through.
func TestOrphanedOutOfLaneOptOutIsReported(t *testing.T) {
	files := map[string]string{"volume/renamed.rs": `
#[tokio::test]
// allowed-out-of-lane-fixture-cell: it used to be a bench
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_reads_a_file_over_the_session() {}
`}
	scan := scanFixtureLaneCoverage(files)
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

// The SFTP half of the lane. A gated SFTP cell is exactly the same failure as a
// gated SMB one: it needs a fixture, the lane has to select it, and everything
// stays green while it never runs.
func TestFindOutOfLaneSftpCells(t *testing.T) {
	tests := []struct {
		name  string
		files map[string]string
		want  []string
	}{
		{
			name: "a gated SFTP cell carrying its prefix is in the lane",
			files: map[string]string{"volume/listing.rs": `
#[tokio::test]
#[ignore = "Requires the Docker SFTP fixture (./apps/desktop/test/sftp-servers/start.sh)"]
async fn sftp_integration_lists_a_large_directory() {}
`},
		},
		{
			name: "a gated SFTP cell without the prefix is a finding",
			files: map[string]string{"volume/listing.rs": `
#[tokio::test]
#[ignore = "Requires the Docker SFTP fixture (./apps/desktop/test/sftp-servers/start.sh)"]
async fn lists_a_large_directory() {}
`},
			want: []string{"volume/listing.rs:4 lists_a_large_directory"},
		},
		{
			name: "the compose project name gates just as well as the start script",
			files: map[string]string{"volume/rename.rs": `
#[tokio::test]
#[ignore = "Requires the sftp-fixture-openssh container"]
async fn refuses_to_clobber_an_existing_destination() {}
`},
			want: []string{"volume/rename.rs:4 refuses_to_clobber_an_existing_destination"},
		},
		{
			// The prefixes aren't interchangeable: an SFTP cell wearing the SMB
			// prefix would run, but it names the wrong fixture to every reader.
			name: "an SFTP cell wearing the SMB prefix is a finding",
			files: map[string]string{"volume/mixed.rs": `
#[tokio::test]
#[ignore = "Requires the Docker SFTP fixture (./apps/desktop/test/sftp-servers/start.sh)"]
async fn smb_integration_lists_a_large_directory() {}
`},
			want: []string{"volume/mixed.rs:4 smb_integration_lists_a_large_directory"},
		},
		{
			name: "a deliberately out-of-lane SFTP cell says so and is left alone",
			files: map[string]string{"volume/bench.rs": `
#[tokio::test]
// allowed-out-of-lane-fixture-cell: a throughput harness, not an assertion; run on demand
#[ignore = "Measurement harness: needs the Docker SFTP fixture (./apps/desktop/test/sftp-servers/start.sh)"]
async fn windowed_read_against_wall_clock() {}
`},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var got []string
			for _, cell := range scanFixtureLaneCoverage(tt.files).stranded {
				got = append(got, cell.String())
			}
			if strings.Join(got, " | ") != strings.Join(tt.want, " | ") {
				t.Errorf("scanFixtureLaneCoverage = %v, want %v", got, tt.want)
			}
		})
	}
}

// Selection is the whole point: a prefix this check enforces that the lane's
// filter doesn't select on would guard nothing.
func TestFixtureLaneFilterCarriesEveryEnforcedPrefix(t *testing.T) {
	filter := fixtureIntegrationFilter("")
	for _, fixture := range laneFixtures {
		if !strings.Contains(filter, fixture.lanePrefix) {
			t.Errorf("the lane filters on %q, which doesn't carry %q", filter, fixture.lanePrefix)
		}
	}
}
