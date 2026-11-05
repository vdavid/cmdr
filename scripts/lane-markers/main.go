// Lane markers: lets a CI lane skip when the exact set of files it reads already passed in an
// earlier trusted run, so a follow-up push, a rebase, or a re-land doesn't rerun lanes whose
// inputs didn't change.
//
// A lane's hash covers the tracked files its dorny/paths-filter filters match at the commit (the
// same matching dorny does), its definition (the workflow file it sits in, every local `uses:`
// target it reaches), the toolchain pins, and the check runner. A lane that passes saves an
// Actions cache entry keyed `lane-green-<lane>-<hash>`; a later pull request run that finds that
// key in a scope it trusts skips the lane, and `CI OK` re-checks every such skip. Trust argument
// and wiring: `CLAUDE.md` and `DETAILS.md` here.
//
// # Usage
//
//	lane-markers hash [-lane ID] [-files]      print each lane's marker key (and its inputs)
//	lane-markers plan   <context flags>        the `changes` job: decide every lane, write `lanes=<json>`
//	lane-markers verify <context flags> -needs JSON   the `CI OK` job: fail on any unproven skip
//
// Context flags: -event, -ref, -repo, -head-repo, -run-all, -main-ref, -author-type, -author.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	var err error
	switch os.Args[1] {
	case "hash":
		err = runHash(os.Args[2:])
	case "plan":
		err = runPlan(os.Args[2:])
	case "verify":
		err = runVerify(os.Args[2:])
	default:
		usage()
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "lane-markers:", err)
		os.Exit(1)
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: lane-markers hash|plan|verify [flags] (see main.go)")
	os.Exit(2)
}

type commonFlags struct {
	dir, rev string
	ctx      runContext
}

func addCommon(fs *flag.FlagSet, withContext bool) *commonFlags {
	c := &commonFlags{}
	fs.StringVar(&c.dir, "dir", ".", "a directory inside the repo")
	fs.StringVar(&c.rev, "rev", "HEAD", "the commit whose tree to hash")
	if withContext {
		fs.StringVar(&c.ctx.event, "event", "", "github.event_name")
		fs.StringVar(&c.ctx.ref, "ref", "", "github.ref")
		fs.StringVar(&c.ctx.repo, "repo", "", "github.repository")
		fs.StringVar(&c.ctx.headRepo, "head-repo", "", "github.event.pull_request.head.repo.full_name (empty off PRs)")
		fs.BoolVar(&c.ctx.runAll, "run-all", false, "this is a run_all dispatch")
		fs.StringVar(&c.ctx.mainRef, "main-ref", "refs/heads/main", "the default branch's ref")
		fs.StringVar(&c.ctx.authorType, "author-type", "", "github.event.pull_request.user.type (empty off PRs)")
		fs.StringVar(&c.ctx.author, "author", "", "github.event.pull_request.user.login (empty off PRs)")
	}
	return c
}

// load reads the commit's tree, the lane config, and every lane's hash.
func (c *commonFlags) load() (*config, map[string]string, map[string][]entry, error) {
	t, err := loadGitTree(c.dir, c.rev)
	if err != nil {
		return nil, nil, nil, err
	}
	cfg, err := loadConfig(t)
	if err != nil {
		return nil, nil, nil, err
	}
	hashes, inputs := map[string]string{}, map[string][]entry{}
	for _, l := range cfg.lanes {
		in := laneInputs(t, cfg, l)
		inputs[l.id], hashes[l.id] = in, laneHash(l.id, in)
	}
	return cfg, hashes, inputs, nil
}

func runHash(args []string) error {
	fs := flag.NewFlagSet("hash", flag.ExitOnError)
	c := addCommon(fs, false)
	only := fs.String("lane", "", "only this lane")
	files := fs.Bool("files", false, "also list each lane's input files")
	_ = fs.Parse(args)
	cfg, hashes, inputs, err := c.load()
	if err != nil {
		return err
	}
	if len(cfg.lanes) == 0 {
		fmt.Fprintf(os.Stderr, "No job in %s reads its entry of `needs.%s.outputs.%s` yet, so there are no lanes.\n", cfg.workflow, cfg.changesJob, laneOutput)
	}
	for _, l := range cfg.lanes {
		if *only != "" && l.id != *only {
			continue
		}
		fmt.Printf("%s\t%d files\t%s\n", l.id, len(inputs[l.id]), markerKey(l.id, hashes[l.id]))
		if *files {
			for _, e := range inputs[l.id] {
				fmt.Printf("  %s\n", e.path)
			}
		}
	}
	return nil
}

func runPlan(args []string) error {
	fs := flag.NewFlagSet("plan", flag.ExitOnError)
	c := addCommon(fs, true)
	_ = fs.Parse(args)
	cfg, hashes, _, err := c.load()
	if err != nil {
		return err
	}
	markers, err := listTrusted(ghLister{repo: c.ctx.repo}, c.ctx)
	if err != nil {
		// Fail toward running: no markers means no lane skips on one.
		fmt.Printf("::warning::Couldn't list lane markers, so every lane runs on its filters: %v\n", err)
		markers = nil
	}
	plan := map[string]decision{}
	for _, l := range cfg.lanes {
		d := decide(l.id, hashes[l.id], c.ctx, markers)
		plan[l.id] = d
		fmt.Printf("%s: %s (%s)\n", l.id, d.reason, d.Key)
	}
	out, err := json.Marshal(plan)
	if err != nil {
		return err
	}
	return writeOutput(laneOutput, string(out))
}

func runVerify(args []string) error {
	fs := flag.NewFlagSet("verify", flag.ExitOnError)
	c := addCommon(fs, true)
	needsJSON := fs.String("needs", "", "toJSON(needs) of the CI OK job")
	_ = fs.Parse(args)
	var needs map[string]jobNeed
	if err := json.Unmarshal([]byte(*needsJSON), &needs); err != nil {
		return fmt.Errorf("-needs isn't toJSON(needs): %w", err)
	}
	cfg, hashes, _, err := c.load()
	if err != nil {
		return err
	}
	in := verifyInput{lanes: cfg.lanes, hashes: hashes, needs: needs, ctx: c.ctx}
	if c.ctx.trustedRefs() != nil {
		in.markers, in.listErr = listTrusted(ghLister{repo: c.ctx.repo}, c.ctx)
	}
	if bad := verifyLanes(in); len(bad) > 0 {
		fmt.Println("These lanes skipped without proof that their inputs passed:")
		for _, b := range bad {
			fmt.Println("  " + b)
		}
		return fmt.Errorf("%d unproven lane skip(s)", len(bad))
	}
	fmt.Printf("Every skipped lane is a path-filter skip or a verified one (%d lanes).\n", len(cfg.lanes))
	return nil
}

// writeOutput appends `name=value` to $GITHUB_OUTPUT, or prints it off CI.
func writeOutput(name, value string) error {
	p := os.Getenv("GITHUB_OUTPUT")
	if p == "" {
		fmt.Printf("%s=%s\n", name, value)
		return nil
	}
	f, err := os.OpenFile(p, os.O_APPEND|os.O_WRONLY|os.O_CREATE, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()
	_, err = fmt.Fprintf(f, "%s=%s\n", name, value)
	return err
}
