package main

import (
	"fmt"
	"path"
	"regexp"
	"sort"
	"strings"

	"go.yaml.in/yaml/v3"
)

const workflowsDir = ".github/workflows"

// laneOutput is the `changes` job output carrying the plan (`lane-markers plan`'s JSON, one
// `decision` per lane). A job becomes a lane by reading its own marker key,
// `fromJSON(needs.changes.outputs.lanes)['<job id>'].key`, which only the job that saves the
// marker needs. Any job may read a lane's `.verified` (a prerequisite job following the lane it
// feeds).
const laneOutput = "lanes"

// config is what lane-markers reads from the workflows at one commit: the dorny filters, and the
// jobs that opted in as lanes.
type config struct {
	workflow   string // repo-relative path of the workflow hosting the filters and the lanes
	changesJob string // id of the job running dorny/paths-filter
	quant      quantifier
	filters    map[string]filter
	lanes      []lane // sorted by id
}

// lane is one opted-in job. Its inputs are the files its filters match plus its definition: the
// workflow file it sits in and every local `uses:` target it reaches, both counting the jobs it
// `needs` (they build what it tests), plus `alwaysInputs`.
type lane struct {
	id       string
	gate     []string // the filter outputs its `if:` reads: when one is true, a skip needs a marker
	filters  []string // every filter output it or a job it needs reads: the hashed inputs
	defFiles []string // exact files (workflows, action metadata)
	defDirs  []string // local composite actions: every tracked file under them
}

// changesJobID is the job every lane `needs` for its gate. Other workflows may run dorny too
// (`codeql.yml` does, for its own push gate); only the step in a job by this id defines lanes.
const changesJobID = "changes"

// loadConfig finds the one dorny/paths-filter step in a `changes` job across the workflows,
// parses its filters, and reads the lanes from the jobs of that same workflow. It's robust to lanes moving into reusable
// `<lane>-lane.yml` files: the calling job stays next to `changes`, and the reusable file is a
// local `uses:` target, so it lands in the lane's definition.
func loadConfig(t tree) (*config, error) {
	var found []*config
	var docs = map[string]*yaml.Node{}
	for _, e := range t.entries() {
		if path.Dir(e.path) != workflowsDir || !(strings.HasSuffix(e.path, ".yml") || strings.HasSuffix(e.path, ".yaml")) {
			continue
		}
		doc, err := parseYAMLFile(t, e.path)
		if err != nil {
			return nil, err
		}
		docs[e.path] = doc
		for jobID, job := range mapPairs(mapGet(doc, "jobs")) {
			if jobID != changesJobID {
				continue
			}
			for _, step := range seqItems(mapGet(job, "steps")) {
				uses := scalar(mapGet(step, "uses"))
				if !strings.HasPrefix(uses, "dorny/paths-filter@") {
					continue
				}
				c, err := parseFilterStep(step)
				if err != nil {
					return nil, fmt.Errorf("%s job %s: %w", e.path, jobID, err)
				}
				c.workflow, c.changesJob = e.path, jobID
				found = append(found, c)
			}
		}
	}
	if len(found) != 1 {
		return nil, fmt.Errorf("want exactly one dorny/paths-filter step in a `%s` job under %s, found %d", changesJobID, workflowsDir, len(found))
	}
	c := found[0]
	lanes, err := parseLanes(t, c, docs[c.workflow])
	if err != nil {
		return nil, err
	}
	c.lanes = lanes
	return c, nil
}

func parseYAMLFile(t tree, p string) (*yaml.Node, error) {
	raw, err := t.read(p)
	if err != nil {
		return nil, err
	}
	var doc yaml.Node
	if err := yaml.Unmarshal(raw, &doc); err != nil {
		return nil, fmt.Errorf("parse %s: %w", p, err)
	}
	if doc.Kind == yaml.DocumentNode && len(doc.Content) == 1 {
		return doc.Content[0], nil
	}
	return &doc, nil
}

func parseFilterStep(step *yaml.Node) (*config, error) {
	with := mapGet(step, "with")
	q, err := parseQuantifier(scalar(mapGet(with, "predicate-quantifier")))
	if err != nil {
		return nil, err
	}
	filters, err := parseFilters(scalar(mapGet(with, "filters")))
	if err != nil {
		return nil, err
	}
	return &config{quant: q, filters: filters}, nil
}

// parseFilters reads dorny's `filters` YAML: each name maps to a list of patterns, where a nested
// list (a YAML anchor's reuse) flattens in place. Status-scoped rules (`added|modified: ...`) are
// refused: a tree hash has no change status to scope by.
func parseFilters(src string) (map[string]filter, error) {
	if strings.TrimSpace(src) == "" {
		return nil, fmt.Errorf("the dorny step has no inline `filters`")
	}
	var doc yaml.Node
	if err := yaml.Unmarshal([]byte(src), &doc); err != nil {
		return nil, fmt.Errorf("parse filters: %w", err)
	}
	var root *yaml.Node
	if doc.Kind == yaml.DocumentNode && len(doc.Content) == 1 {
		root = deref(doc.Content[0])
	}
	if root == nil || root.Kind != yaml.MappingNode {
		return nil, fmt.Errorf("filters must be a mapping of name to patterns (a filters file path isn't supported)")
	}
	out := map[string]filter{}
	for name, items := range mapPairs(root) {
		f := filter{name: name}
		var add func(n *yaml.Node) error
		add = func(n *yaml.Node) error {
			n = deref(n)
			switch n.Kind {
			case yaml.ScalarNode:
				g, err := compileGlob(n.Value)
				if err != nil {
					return err
				}
				f.patterns = append(f.patterns, g)
			case yaml.SequenceNode:
				for _, c := range n.Content {
					if err := add(c); err != nil {
						return err
					}
				}
			default:
				return fmt.Errorf("status-scoped or non-string rule (line %d)", n.Line)
			}
			return nil
		}
		if err := add(items); err != nil {
			return nil, fmt.Errorf("filter %s: %w", name, err)
		}
		out[name] = f
	}
	return out, nil
}

// jobFacts is what one job of the lanes' workflow says about lanes.
type jobFacts struct {
	node        *yaml.Node
	gate        []string        // filter outputs its `if:` reads
	filters     map[string]bool // filter outputs it reads anywhere
	keyRefs     []string        // lane ids whose `.key` it reads
	verdictRefs []string        // lane ids whose other plan fields it reads
	needs       []string
}

// refPatterns match, in a job's expressions, a filter output and a lane plan field.
type refPatterns struct {
	output, plan *regexp.Regexp
}

func newRefPatterns(changesJob string) refPatterns {
	outputs := `needs\.` + regexp.QuoteMeta(changesJob) + `\.outputs\.`
	return refPatterns{
		output: regexp.MustCompile(outputs + `([A-Za-z0-9_-]+)`),
		plan:   regexp.MustCompile(`fromJSON\(\s*` + outputs + laneOutput + `\s*\)\s*\[\s*'([^']*)'\s*\]\s*(?:\.\s*([A-Za-z_]+))?`),
	}
}

func readJobFacts(c *config, p refPatterns, jobID string, job *yaml.Node) (*jobFacts, error) {
	f := &jobFacts{node: job, filters: map[string]bool{}}
	for _, s := range scalars(job) {
		for _, m := range p.plan.FindAllStringSubmatch(s, -1) {
			switch m[2] {
			case "key":
				f.keyRefs = append(f.keyRefs, m[1])
			case "verified", "hash", "ref":
				f.verdictRefs = append(f.verdictRefs, m[1])
			default:
				return nil, fmt.Errorf("job %s reads `['%s'].%s` of the lane plan; its fields are key, verified, hash, and ref", jobID, m[1], m[2])
			}
		}
		for _, name := range filterRefs(c, p, s) {
			f.filters[name] = true
		}
	}
	gate := map[string]bool{}
	for _, name := range filterRefs(c, p, scalar(mapGet(job, "if"))) {
		if !gate[name] {
			gate[name] = true
			f.gate = append(f.gate, name)
		}
	}
	sort.Strings(f.gate)
	if n := mapGet(job, "needs"); n != nil {
		if s := scalar(n); s != "" {
			f.needs = []string{s}
		}
		for _, item := range seqItems(n) {
			f.needs = append(f.needs, scalar(item))
		}
	}
	return f, nil
}

// filterRefs lists the filter outputs an expression reads (other `changes` outputs aside).
func filterRefs(c *config, p refPatterns, s string) []string {
	var out []string
	for _, m := range p.output.FindAllStringSubmatch(s, -1) {
		if _, ok := c.filters[m[1]]; ok {
			out = append(out, m[1])
		}
	}
	return out
}

func parseLanes(t tree, c *config, doc *yaml.Node) ([]lane, error) {
	p := newRefPatterns(c.changesJob)
	jobs := map[string]*jobFacts{}
	for jobID, job := range mapPairs(mapGet(doc, "jobs")) {
		f, err := readJobFacts(c, p, jobID, job)
		if err != nil {
			return nil, err
		}
		jobs[jobID] = f
	}

	isLane := map[string]bool{}
	for jobID, f := range jobs {
		for _, id := range f.keyRefs {
			// Only the job that saves a lane's marker reads its key, so a copy-pasted key can't
			// save one lane's marker from another lane's job.
			if id != jobID {
				return nil, fmt.Errorf("job %s reads lane %q's marker key; a job saves only its own (`['%s'].key`)", jobID, id, jobID)
			}
			isLane[jobID] = true
		}
	}
	var lanes []lane
	for jobID, f := range jobs {
		for _, id := range f.verdictRefs {
			if !isLane[id] {
				return nil, fmt.Errorf("job %s reads the plan of %q, which isn't a lane (no job reads its `.key`)", jobID, id)
			}
		}
		if !isLane[jobID] {
			continue
		}
		l, err := buildLane(t, c, jobs, jobID)
		if err != nil {
			return nil, fmt.Errorf("lane %s: %w", jobID, err)
		}
		lanes = append(lanes, l)
	}
	sort.Slice(lanes, func(i, j int) bool { return lanes[i].id < lanes[j].id })
	return lanes, nil
}

// buildLane gathers a lane's filters and definition from its own job and, transitively, every job
// it `needs` (but `changes`): a prerequisite like the E2E base image builds part of what the lane
// tests, so its filters and actions are the lane's inputs too.
func buildLane(t tree, c *config, jobs map[string]*jobFacts, laneID string) (lane, error) {
	l := lane{id: laneID, defFiles: []string{c.workflow}}
	filters := map[string]bool{}
	seenJobs := map[string]bool{}
	seenUses := map[string]bool{c.workflow: true}
	var visit func(jobID string) error
	visit = func(jobID string) error {
		if seenJobs[jobID] || jobID == c.changesJob {
			return nil
		}
		seenJobs[jobID] = true
		f, ok := jobs[jobID]
		if !ok {
			return fmt.Errorf("needs a job %q that doesn't exist", jobID)
		}
		for name := range f.filters {
			filters[name] = true
		}
		if err := addLocalUses(t, &l, f.node, seenUses); err != nil {
			return err
		}
		for _, n := range f.needs {
			if err := visit(n); err != nil {
				return err
			}
		}
		return nil
	}
	if err := visit(laneID); err != nil {
		return l, err
	}
	if f := jobs[laneID]; f != nil {
		l.gate = f.gate
	}
	if len(l.gate) == 0 {
		return l, fmt.Errorf("its `if:` reads no filter output (`needs.%s.outputs.<filter>`), so nothing defines its inputs", c.changesJob)
	}
	for name := range filters {
		l.filters = append(l.filters, name)
	}
	sort.Strings(l.filters)
	sort.Strings(l.defFiles)
	sort.Strings(l.defDirs)
	return l, nil
}

// addLocalUses adds every local `uses: ./...` target under n to the lane's definition, following
// reusable workflows and composite actions into their own local `uses:`.
func addLocalUses(t tree, l *lane, n *yaml.Node, seen map[string]bool) error {
	for _, u := range usesValues(n) {
		if !strings.HasPrefix(u, "./") {
			continue // a pinned remote action or `docker://`: its SHA is in the hashed file's text
		}
		target := strings.TrimSuffix(strings.TrimPrefix(u, "./"), "/")
		if seen[target] {
			continue
		}
		seen[target] = true
		var next *yaml.Node
		switch {
		case hasFile(t, target):
			l.defFiles = append(l.defFiles, target)
			doc, err := parseYAMLFile(t, target)
			if err != nil {
				return err
			}
			next = doc
		case hasDir(t, target):
			l.defDirs = append(l.defDirs, target)
			for _, meta := range []string{target + "/action.yml", target + "/action.yaml"} {
				if hasFile(t, meta) {
					doc, err := parseYAMLFile(t, meta)
					if err != nil {
						return err
					}
					next = doc
				}
			}
		default:
			return fmt.Errorf("local `uses: %s` isn't a tracked file or directory", u)
		}
		if next != nil {
			if err := addLocalUses(t, l, next, seen); err != nil {
				return err
			}
		}
	}
	return nil
}

// --- yaml.Node helpers ---

func deref(n *yaml.Node) *yaml.Node {
	for n != nil && n.Kind == yaml.AliasNode {
		n = n.Alias
	}
	return n
}

func mapGet(n *yaml.Node, key string) *yaml.Node {
	n = deref(n)
	if n == nil || n.Kind != yaml.MappingNode {
		return nil
	}
	for i := 0; i+1 < len(n.Content); i += 2 {
		if n.Content[i].Value == key {
			return deref(n.Content[i+1])
		}
	}
	return nil
}

// mapPairs yields a mapping's key/value pairs in file order.
func mapPairs(n *yaml.Node) func(yield func(string, *yaml.Node) bool) {
	return func(yield func(string, *yaml.Node) bool) {
		n = deref(n)
		if n == nil || n.Kind != yaml.MappingNode {
			return
		}
		for i := 0; i+1 < len(n.Content); i += 2 {
			if !yield(n.Content[i].Value, deref(n.Content[i+1])) {
				return
			}
		}
	}
}

func seqItems(n *yaml.Node) []*yaml.Node {
	n = deref(n)
	if n == nil || n.Kind != yaml.SequenceNode {
		return nil
	}
	out := make([]*yaml.Node, 0, len(n.Content))
	for _, c := range n.Content {
		out = append(out, deref(c))
	}
	return out
}

func scalar(n *yaml.Node) string {
	n = deref(n)
	if n == nil || n.Kind != yaml.ScalarNode {
		return ""
	}
	return n.Value
}

// scalars lists every scalar (keys and values) under n.
func scalars(n *yaml.Node) []string {
	n = deref(n)
	if n == nil {
		return nil
	}
	if n.Kind == yaml.ScalarNode {
		return []string{n.Value}
	}
	var out []string
	for _, c := range n.Content {
		out = append(out, scalars(c)...)
	}
	return out
}

// usesValues lists the value of every `uses:` key under n.
func usesValues(n *yaml.Node) []string {
	n = deref(n)
	if n == nil {
		return nil
	}
	var out []string
	if n.Kind == yaml.MappingNode {
		for i := 0; i+1 < len(n.Content); i += 2 {
			if n.Content[i].Value == "uses" {
				if v := scalar(n.Content[i+1]); v != "" {
					out = append(out, v)
				}
			}
		}
	}
	for _, c := range n.Content {
		out = append(out, usesValues(c)...)
	}
	return out
}
