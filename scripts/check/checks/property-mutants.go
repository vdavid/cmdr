package checks

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// The shared vocabulary of the two property-mutant checks: the record format,
// its loader and validator, the property kinds, and the allowlist. A property
// (a fuzz target today) is only worth something if a plausible bug makes it
// fail, so every property carries at least one recorded one-line change that
// does. `desktop-rust-property-mutant-records` validates the records on every
// commit; `desktop-rust-property-mutants` applies each one and runs its catcher.
// Format and how to add a record: `DETAILS.md` § "Property mutants".

// mutantRecordSuffix names a record file. One sits beside the properties it
// covers, anywhere in the tree.
const mutantRecordSuffix = ".mutants.json"

// propertyMutantsAllowlistRelPath lists the properties with no record yet, each
// with a reason. Local runs drop an entry once its property gains a record.
const propertyMutantsAllowlistRelPath = "scripts/check/checks/property-mutants-allowlist.json"

// mutantRecordFile is the on-disk shape of one `*.mutants.json`.
type mutantRecordFile struct {
	Mutants []mutantRecord `json:"mutants"`
}

// mutantRecord is one breaking change and how it gets caught.
type mutantRecord struct {
	// Property is `<kind>:<name>`, for example `fuzz:archive_entry_name`.
	Property string `json:"property"`
	// Why names the bug the mutant stands for, in one line.
	Why string `json:"why"`
	// File is the repo-relative file the mutant edits.
	File string `json:"file"`
	// Find is one line's worth of text that occurs exactly once in File, and
	// Replace is what the mutant turns it into.
	Find    string `json:"find"`
	Replace string `json:"replace"`
	// Input is the repo-relative input that reaches the property, for kinds that
	// take one (fuzz). A `.gz` input is decompressed first.
	Input string `json:"input,omitempty"`
	// Expect is text the failure must print, so the mutant is caught by THIS
	// property and not by some other panic on the way.
	Expect string `json:"expect"`

	// source is the record file it came from, for messages.
	source string
}

// label names a record in messages: the record file plus the property.
func (r mutantRecord) label() string {
	return fmt.Sprintf("%s (%s)", r.source, r.Property)
}

// kindAndName splits the property ID.
func (r mutantRecord) kindAndName() (string, string) {
	kind, name, _ := strings.Cut(r.Property, ":")
	return kind, name
}

// kindIn finds the record's kind; ok is false for a kind nobody registered.
func (r mutantRecord) kindIn(kinds []propertyKind) (propertyKind, bool) {
	name, _ := r.kindAndName()
	if kind := propertyKindNamed(kinds, name); kind != nil {
		return *kind, true
	}
	return propertyKind{}, false
}

// mutantRun is what one run of a record's catcher said.
type mutantRun struct {
	// failed means the property went red (a panic, a sanitizer report, a hang).
	failed bool
	output string
}

// propertyKind is one family of properties. Adding a family (Kani proofs,
// Shuttle tests) means one of these in its own `property-mutants-<kind>.go`,
// listed in `propertyKinds`.
type propertyKind struct {
	name string
	// properties lists every property of this kind in the tree, by name.
	properties func(rootDir string) ([]string, error)
	// checkRecord returns the problems with the kind's own fields of a record.
	checkRecord func(rootDir string, r mutantRecord) []string
	// prepare installs what run needs, once per slow run.
	prepare func() error
	// run builds tree as it stands and runs the record's catcher against it. An
	// error means it couldn't run at all (the mutant doesn't build, a tool is
	// missing); a property going red is `mutantRun.failed`.
	run func(tree string, r mutantRecord) (mutantRun, error)
}

// propertyKinds is every kind the checks know.
var propertyKinds = []propertyKind{fuzzPropertyKind, shuttlePropertyKind}

// propertyKindNamed finds a kind, or nil.
func propertyKindNamed(kinds []propertyKind, name string) *propertyKind {
	for i := range kinds {
		if kinds[i].name == name {
			return &kinds[i]
		}
	}
	return nil
}

// discoverProperties returns every property ID in the tree, sorted.
func discoverProperties(rootDir string, kinds []propertyKind) ([]string, error) {
	var ids []string
	for _, kind := range kinds {
		names, err := kind.properties(rootDir)
		if err != nil {
			return nil, fmt.Errorf("list the %s properties: %w", kind.name, err)
		}
		for _, name := range names {
			ids = append(ids, kind.name+":"+name)
		}
	}
	sort.Strings(ids)
	return ids, nil
}

// mutantRecordFiles lists every record file git knows about, tracked or not yet
// added, so a record written before its first commit is checked too.
func mutantRecordFiles(rootDir string) ([]string, error) {
	files, err := corpus.GitLsFiles(rootDir, "-co", "--exclude-standard")
	if err != nil {
		return nil, err
	}
	var out []string
	for _, rel := range files {
		if strings.HasSuffix(rel, mutantRecordSuffix) && fileExists(filepath.Join(rootDir, rel)) {
			out = append(out, rel)
		}
	}
	sort.Strings(out)
	return out, nil
}

// loadMutantRecords reads every record file and validates each record against
// the tree: a record that no longer applies is a problem, so records can't rot.
func loadMutantRecords(rootDir string, kinds []propertyKind) ([]mutantRecord, []string, error) {
	recordFiles, err := mutantRecordFiles(rootDir)
	if err != nil {
		return nil, nil, err
	}
	properties, err := discoverProperties(rootDir, kinds)
	if err != nil {
		return nil, nil, err
	}
	known := map[string]bool{}
	for _, id := range properties {
		known[id] = true
	}

	var records []mutantRecord
	var problems []string
	for _, rel := range recordFiles {
		parsed, err := parseMutantRecordFile(filepath.Join(rootDir, rel))
		if err != nil {
			problems = append(problems, fmt.Sprintf("%s: %v", rel, err))
			continue
		}
		if len(parsed.Mutants) == 0 {
			problems = append(problems, fmt.Sprintf("%s: holds no mutants; delete it, or record one", rel))
		}
		for _, r := range parsed.Mutants {
			r.source = rel
			for _, p := range checkMutantRecord(rootDir, r, kinds, known) {
				problems = append(problems, fmt.Sprintf("%s: %s", r.label(), p))
			}
			records = append(records, r)
		}
	}
	return records, problems, nil
}

// parseMutantRecordFile decodes one record file, refusing unknown fields so a
// misspelled one fails rather than going unread.
func parseMutantRecordFile(path string) (mutantRecordFile, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return mutantRecordFile{}, err
	}
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.DisallowUnknownFields()
	var parsed mutantRecordFile
	if err := dec.Decode(&parsed); err != nil {
		return mutantRecordFile{}, fmt.Errorf("not a record file: %w", err)
	}
	return parsed, nil
}

// checkMutantRecord returns everything wrong with one record.
func checkMutantRecord(rootDir string, r mutantRecord, kinds []propertyKind, known map[string]bool) []string {
	var problems []string
	kindName, name := r.kindAndName()
	kind := propertyKindNamed(kinds, kindName)
	switch {
	case kind == nil || name == "":
		problems = append(problems, fmt.Sprintf("`property` %q isn't `<kind>:<name>` with a known kind", r.Property))
	case !known[r.Property]:
		problems = append(problems, fmt.Sprintf("there's no %s property named %q any more", kindName, name))
	}
	for _, field := range []struct{ key, value string }{{"why", r.Why}, {"find", r.Find}, {"expect", r.Expect}} {
		if strings.TrimSpace(field.value) == "" {
			problems = append(problems, fmt.Sprintf("`%s` is empty", field.key))
		}
	}
	for _, field := range []struct{ key, value string }{
		{"why", r.Why}, {"find", r.Find}, {"replace", r.Replace}, {"expect", r.Expect},
	} {
		if strings.Contains(field.value, "\n") {
			problems = append(problems, fmt.Sprintf("`%s` spans lines; a mutant is a one-line change", field.key))
		}
	}
	if r.Find == r.Replace {
		problems = append(problems, "`replace` equals `find`, so the mutant changes nothing")
	}
	problems = append(problems, checkMutantTarget(rootDir, r)...)
	if kind != nil {
		problems = append(problems, kind.checkRecord(rootDir, r)...)
	}
	return problems
}

// checkMutantTarget checks that the record's `find` still names one spot in
// its file.
func checkMutantTarget(rootDir string, r mutantRecord) []string {
	if !isRepoRelative(r.File) {
		return []string{fmt.Sprintf("`file` %q isn't a repo-relative path", r.File)}
	}
	data, err := os.ReadFile(filepath.Join(rootDir, filepath.FromSlash(r.File)))
	if err != nil {
		return []string{fmt.Sprintf("`file` %s can't be read: %v", r.File, err)}
	}
	if r.Find == "" {
		return nil
	}
	switch n := strings.Count(string(data), r.Find); n {
	case 1:
		return nil
	case 0:
		return []string{fmt.Sprintf("`find` no longer occurs in %s; re-aim the mutant at the code as it is now", r.File)}
	default:
		return []string{fmt.Sprintf("`find` occurs %d times in %s; widen it until it names one spot", n, r.File)}
	}
}

// isRepoRelative reports whether rel is a forward-slashed path inside the
// repo, so nothing a record names can reach outside the tree it's applied in.
func isRepoRelative(rel string) bool {
	return rel != "" && !strings.Contains(rel, "\\") && filepath.IsLocal(filepath.FromSlash(rel))
}

// applyMutant edits the record's file under tree and returns the function that
// puts it back. The restore writes afresh, so its mtime is newer than any build
// of the mutant and cargo rebuilds.
func applyMutant(tree string, r mutantRecord) (func() error, error) {
	path := filepath.Join(tree, filepath.FromSlash(r.File))
	original, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	if n := bytes.Count(original, []byte(r.Find)); n != 1 {
		return nil, fmt.Errorf("`find` occurs %d times in the mirrored %s", n, r.File)
	}
	info, err := os.Stat(path)
	if err != nil {
		return nil, err
	}
	mutated := bytes.Replace(original, []byte(r.Find), []byte(r.Replace), 1)
	if err := os.WriteFile(path, mutated, info.Mode().Perm()); err != nil {
		return nil, err
	}
	return func() error { return os.WriteFile(path, original, info.Mode().Perm()) }, nil
}

// propertyMutantsAllowlist is the on-disk shape of the allowlist.
type propertyMutantsAllowlist struct {
	Comment    string                        `json:"$comment,omitempty"`
	Unrecorded map[string]unrecordedProperty `json:"unrecorded"`
}

// unrecordedProperty is one property allowed to go without a record.
type unrecordedProperty struct {
	Reason string `json:"reason"`
}

// loadPropertyMutantsAllowlist reads the allowlist. A missing file is an empty
// one; an unreadable one is an error, so a typo can't empty it silently.
func loadPropertyMutantsAllowlist(rootDir string) (propertyMutantsAllowlist, error) {
	list := propertyMutantsAllowlist{Unrecorded: map[string]unrecordedProperty{}}
	data, err := os.ReadFile(filepath.Join(rootDir, propertyMutantsAllowlistRelPath))
	if os.IsNotExist(err) {
		return list, nil
	}
	if err != nil {
		return list, err
	}
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&list); err != nil {
		return list, fmt.Errorf("%s: %w", propertyMutantsAllowlistRelPath, err)
	}
	if list.Unrecorded == nil {
		list.Unrecorded = map[string]unrecordedProperty{}
	}
	return list, nil
}

// propertyCoverage is the verdict on which properties have records.
type propertyCoverage struct {
	// unrecorded are properties with neither a record nor an allowlist entry.
	unrecorded []string
	// stale are allowlist entries to drop, each with why.
	stale []string
	// reasonless are allowlist entries with no reason.
	reasonless []string
}

// judgePropertyCoverage compares the properties against the records and the
// allowlist, and drops the stale entries from list.
func judgePropertyCoverage(properties []string, records []mutantRecord, list *propertyMutantsAllowlist) propertyCoverage {
	recorded := map[string]bool{}
	for _, r := range records {
		recorded[r.Property] = true
	}
	exists := map[string]bool{}
	for _, id := range properties {
		exists[id] = true
	}

	var verdict propertyCoverage
	for _, id := range properties {
		if !recorded[id] {
			if _, ok := list.Unrecorded[id]; !ok {
				verdict.unrecorded = append(verdict.unrecorded, id)
			}
		}
	}
	for _, id := range sortedKeys(list.Unrecorded) {
		switch {
		case !exists[id]:
			delete(list.Unrecorded, id)
			verdict.stale = append(verdict.stale, fmt.Sprintf("removed %s (no such property any more)", id))
		case recorded[id]:
			delete(list.Unrecorded, id)
			verdict.stale = append(verdict.stale, fmt.Sprintf("removed %s (it has a record now)", id))
		case strings.TrimSpace(list.Unrecorded[id].Reason) == "":
			verdict.reasonless = append(verdict.reasonless, id)
		}
	}
	return verdict
}
