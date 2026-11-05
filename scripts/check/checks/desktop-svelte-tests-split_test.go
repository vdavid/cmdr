package checks

import (
	"os"
	"strings"
	"testing"
)

func TestVitestSplitFromEnv(t *testing.T) {
	env := func(vars map[string]string) func(string) string {
		return func(k string) string { return vars[k] }
	}
	cases := []struct {
		name    string
		vars    map[string]string
		want    vitestSplit
		wantErr bool
	}{
		{name: "unset runs the whole suite", vars: nil, want: vitestSplit{}},
		{name: "a shard", vars: map[string]string{VitestShardEnv: "2/4", VitestBlobDirEnv: "/b"}, want: vitestSplit{shard: "2/4", blobDir: "/b"}},
		{name: "the merge", vars: map[string]string{VitestMergeDirEnv: "/m"}, want: vitestSplit{mergeDir: "/m"}},
		{name: "a shard with nowhere to write", vars: map[string]string{VitestShardEnv: "1/4"}, wantErr: true},
		{name: "a blob dir without a shard", vars: map[string]string{VitestBlobDirEnv: "/b"}, wantErr: true},
		{name: "shard and merge at once", vars: map[string]string{VitestShardEnv: "1/4", VitestBlobDirEnv: "/b", VitestMergeDirEnv: "/m"}, wantErr: true},
		{name: "a malformed shard", vars: map[string]string{VitestShardEnv: "four", VitestBlobDirEnv: "/b"}, wantErr: true},
		{name: "an index past the count", vars: map[string]string{VitestShardEnv: "5/4", VitestBlobDirEnv: "/b"}, wantErr: true},
		{name: "a zero index", vars: map[string]string{VitestShardEnv: "0/4", VitestBlobDirEnv: "/b"}, wantErr: true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, err := vitestSplitFromEnv(env(tc.vars))
			if tc.wantErr {
				if err == nil {
					t.Fatalf("expected an error, got %+v", got)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tc.want {
				t.Fatalf("got %+v, want %+v", got, tc.want)
			}
		})
	}
}

func TestNewCoverageRun_SplitArgs(t *testing.T) {
	desktopDir := t.TempDir()

	shard, err := newCoverageRun(desktopDir, vitestSplit{shard: "3/4", blobDir: "/blobs"})
	if err != nil {
		t.Fatalf("newCoverageRun: %v", err)
	}
	defer os.RemoveAll(shard.reportsDir)
	if got := strings.Join(shard.cmd.Args[1:], " "); got != "test:coverage --shard=3/4" {
		t.Errorf("shard args: %q", got)
	}
	if !containsEnv(shard.cmd.Env, "VITEST_BLOB_REPORT=/blobs/blob-3-4.json") {
		t.Errorf("shard env lacks the blob report path: %v", shard.cmd.Env)
	}

	merge, err := newCoverageRun(desktopDir, vitestSplit{mergeDir: "/blobs"})
	if err != nil {
		t.Fatalf("newCoverageRun: %v", err)
	}
	defer os.RemoveAll(merge.reportsDir)
	if got := strings.Join(merge.cmd.Args[1:], " "); got != "test:coverage --merge-reports=/blobs" {
		t.Errorf("merge args: %q", got)
	}
	for _, e := range merge.cmd.Env {
		if strings.HasPrefix(e, "VITEST_BLOB_REPORT=") {
			t.Errorf("a merge must write no blob, env has %q", e)
		}
	}
}
