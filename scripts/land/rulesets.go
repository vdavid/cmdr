package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
)

// rulesets creates or updates the repo's rulesets from `.github/rulesets/*.json`, matching them by
// name, and leaves alone the ones GitHub already has as the file says (`covers`). Without
// `-apply` it only says what it would do. A ruleset with no file here is left alone.
func rulesets(r runner, args []string) error {
	fs := flag.NewFlagSet("rulesets", flag.ExitOnError)
	apply := fs.Bool("apply", false, "create or update the rulesets on GitHub (default: only print the plan)")
	if err := fs.Parse(args); err != nil {
		return err
	}
	files, err := filepath.Glob(filepath.Join(r.dir, ".github", "rulesets", "*.json"))
	if err != nil {
		return err
	}
	var existing []struct {
		ID   int64  `json:"id"`
		Name string `json:"name"`
	}
	if err := r.ghJSON(&existing, "api", fmt.Sprintf("repos/%s/rulesets", repo)); err != nil {
		return err
	}
	ids := make(map[string]int64, len(existing))
	for _, rs := range existing {
		ids[rs.Name] = rs.ID
	}
	pending := 0
	for _, file := range files {
		name, want, err := readRuleset(file)
		if err != nil {
			return err
		}
		if p := placeholderActor(want); p != "" {
			say("%q waits: %s names bypass actor %q, a placeholder. Put the real id in, then -apply (scripts/land/DETAILS.md § The rulesets).", name, filepath.Base(file), p)
			continue
		}
		method, path := "POST", fmt.Sprintf("repos/%s/rulesets", repo)
		if id, ok := ids[name]; ok {
			method, path = "PUT", fmt.Sprintf("%s/%d", path, id)
			var live any
			if err := r.ghJSON(&live, "api", path); err != nil {
				return err
			}
			if covers(want, live) {
				say("%q matches %s", name, filepath.Base(file))
				continue
			}
		}
		pending++
		say("%s %q from %s", map[string]string{"POST": "create", "PUT": "update"}[method], name, filepath.Base(file))
		if !*apply {
			continue
		}
		if _, err := r.gh("api", "-X", method, path, "--input", file); err != nil {
			return err
		}
	}
	if !*apply && pending > 0 {
		say("Nothing changed; add -apply to do it.")
	}
	return nil
}

// covers reports whether GitHub's copy of a ruleset (live) says everything the file (want) says.
// GitHub adds ids and defaults the file leaves out, so extra object keys don't count; arrays must
// match item for item.
func covers(want, live any) bool {
	switch w := want.(type) {
	case map[string]any:
		l, ok := live.(map[string]any)
		if !ok {
			return false
		}
		for k, v := range w {
			if lv, ok := l[k]; !ok || !covers(v, lv) {
				return false
			}
		}
		return true
	case []any:
		l, ok := live.([]any)
		if !ok || len(l) != len(w) {
			return false
		}
		for i := range w {
			if !covers(w[i], l[i]) {
				return false
			}
		}
		return true
	default:
		return want == live
	}
}

// placeholderActor is the first bypass actor id the file gives as a string, like
// "CMDR_AGENTS_APP_ID", or "" when every id is a number. GitHub takes numbers only, so a file with
// a placeholder is one waiting for an id that doesn't exist yet, and `rulesets` leaves it alone
// rather than send GitHub a ruleset it refuses halfway through an `-apply`.
func placeholderActor(rs any) string {
	m, _ := rs.(map[string]any)
	actors, _ := m["bypass_actors"].([]any)
	for _, a := range actors {
		actor, _ := a.(map[string]any)
		if id, ok := actor["actor_id"].(string); ok {
			return id
		}
	}
	return ""
}

// readRuleset reads a ruleset file: its name, and the whole of it for `covers`.
func readRuleset(file string) (string, any, error) {
	data, err := os.ReadFile(file)
	if err != nil {
		return "", nil, err
	}
	var rs map[string]any
	err = json.Unmarshal(data, &rs)
	name, _ := rs["name"].(string)
	if err != nil || name == "" {
		return "", nil, fmt.Errorf("%s: not a ruleset with a name: %v", file, err)
	}
	return name, rs, nil
}
