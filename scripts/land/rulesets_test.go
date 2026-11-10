package main

import (
	"encoding/json"
	"testing"
)

func TestPlaceholderActor(t *testing.T) {
	cases := []struct {
		name, file, want string
	}{
		{"numeric ids only", `{"bypass_actors": [{"actor_id": 5}, {"actor_id": 2740}]}`, ""},
		{"no bypass actors", `{"bypass_actors": []}`, ""},
		{"no bypass key", `{"rules": []}`, ""},
		{"a named placeholder", `{"bypass_actors": [{"actor_id": 5}, {"actor_id": "CMDR_AGENTS_APP_ID"}]}`, "CMDR_AGENTS_APP_ID"},
	}
	for _, c := range cases {
		var rs any
		if err := json.Unmarshal([]byte(c.file), &rs); err != nil {
			t.Fatalf("%s: %v", c.name, err)
		}
		if got := placeholderActor(rs); got != c.want {
			t.Errorf("%s: got %q, want %q", c.name, got, c.want)
		}
	}
}

func TestCovers(t *testing.T) {
	file := `{"name": "main: CI OK required", "bypass_actors": [], "rules": [{"type": "required_status_checks",
		"parameters": {"strict_required_status_checks_policy": false, "required_status_checks": [{"context": "CI OK", "integration_id": 15368}]}}]}`
	cases := []struct {
		name, live string
		want       bool
	}{
		{"identical", file, true},
		{"GitHub adds ids and defaults", `{"id": 1, "name": "main: CI OK required", "source": "vdavid/cmdr", "bypass_actors": [], "rules": [{"type": "required_status_checks",
			"parameters": {"strict_required_status_checks_policy": false, "do_not_enforce_on_create": false, "required_status_checks": [{"context": "CI OK", "integration_id": 15368}]}}]}`, true},
		{"a changed flag", `{"name": "main: CI OK required", "bypass_actors": [], "rules": [{"type": "required_status_checks",
			"parameters": {"strict_required_status_checks_policy": true, "required_status_checks": [{"context": "CI OK", "integration_id": 15368}]}}]}`, false},
		{"an extra bypass actor", `{"name": "main: CI OK required", "bypass_actors": [{"actor_id": 5}], "rules": [{"type": "required_status_checks",
			"parameters": {"strict_required_status_checks_policy": false, "required_status_checks": [{"context": "CI OK", "integration_id": 15368}]}}]}`, false},
		{"a missing rule", `{"name": "main: CI OK required", "bypass_actors": [], "rules": []}`, false},
	}
	var want any
	if err := json.Unmarshal([]byte(file), &want); err != nil {
		t.Fatal(err)
	}
	for _, c := range cases {
		var live any
		if err := json.Unmarshal([]byte(c.live), &live); err != nil {
			t.Fatalf("%s: %v", c.name, err)
		}
		if got := covers(want, live); got != c.want {
			t.Errorf("%s: got %v, want %v", c.name, got, c.want)
		}
	}
}
