package main

import (
	"strings"
	"testing"
)

func TestArchiveFolder(t *testing.T) {
	got, err := archiveFolder("//nas/share/documents/Company AB/evidence/", "v0.55.0")
	if err != nil || got != "//nas/share/documents/Company AB/evidence/v0.55.0" {
		t.Errorf("got %q %v", got, err)
	}
	for _, bad := range []string{"", "/local/path", "//nas", "//nas/share", "smb://nas/share/x", "//nas/share/../x", "//nas//x"} {
		if _, err := archiveFolder(bad, "v0.55.0"); err == nil {
			t.Errorf("%q: want an error", bad)
		}
	}
	if p := pathInShare("//nas/share/documents/Company AB/evidence/v0.55.0"); p != "documents/Company AB/evidence/v0.55.0" {
		t.Errorf("pathInShare: got %q", p)
	}
}

var evidenceFiles = []fileEntry{{Name: "a.json", Size: 10}, {Name: "b.log.gz", Size: 20}}

func TestPlanUploadResumesAndNeverOverwrites(t *testing.T) {
	todo, err := planUpload(evidenceFiles, nil)
	if err != nil || len(todo) != 2 {
		t.Errorf("empty folder: got %v %v", todo, err)
	}
	todo, err = planUpload(evidenceFiles, []fileEntry{{Name: "a.json", Size: 10}})
	if err != nil || len(todo) != 1 || todo[0].Name != "b.log.gz" {
		t.Errorf("half uploaded: got %v %v", todo, err)
	}
	for name, remote := range map[string][]fileEntry{
		"different size": {{Name: "a.json", Size: 11}},
		"stranger file":  {{Name: "c.txt", Size: 1}},
		"subfolder":      {{Name: "a.json", IsDirectory: true}},
	} {
		if _, err := planUpload(evidenceFiles, remote); err == nil || !strings.Contains(err.Error(), "nothing was uploaded") {
			t.Errorf("%s: got %v", name, err)
		}
	}
}

func TestCompareListing(t *testing.T) {
	if err := compareListing(evidenceFiles, evidenceFiles); err != nil {
		t.Error(err)
	}
	if err := compareListing(evidenceFiles, evidenceFiles[:1]); err == nil || !strings.Contains(err.Error(), "b.log.gz") {
		t.Errorf("a missing file should fail: %v", err)
	}
}
