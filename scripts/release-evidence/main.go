// Release evidence: the checks and the CRA record that close every release, steps 12 and 13 of
// `docs/guides/releasing.md` § The release procedure.
//
// `verify` checks the public surface of a published release, and fails on the first thing wrong:
//
//  1. the GitHub release is published and carries every asset, none too small to be real;
//  2. `getcmdr.com/latest.json` names the version (waiting up to `-manifest-wait` for the website
//     deploy) and points each platform at this release's archive, and each archive URL answers 200;
//  3. the `release.yml` runs (the tag push, plus the dispatch in `local` mode) are green on the
//     tagged commit, their `attest` jobs included;
//  4. the full (`run_all`) `ci.yml` run the release rests on, and the CodeQL run on that commit,
//     are green. It prefers a full run on the tagged `chore(release)` commit, else its parent's;
//  5. both `gh attestation verify` checks pass on the Apple Silicon DMG: provenance signed by
//     `release-pipeline.yml` at the tag, and the SBOM binding.
//
// `evidence` runs `verify`, then collects the CRA evidence into `-out/vX.Y.Z`: the SBOMs, the
// provenance bundle, `checksums.txt`, `latest.json`, the jobs JSON and gzipped log of the CI run
// and each release run, one CodeQL SARIF per language, and the attestation output. It uploads the
// folder to the company archive with the `smb2` CLI, lists it back, and prints the release log's
// evidence sub-bullets to paste into the vault. It never writes the vault itself.
//
// # Usage
//
//	./scripts/release-evidence.sh verify 0.55.0
//	./scripts/release-evidence.sh evidence -archive //host/share/path 0.55.0
//	./scripts/release-evidence.sh evidence -no-upload -out /tmp/ev 0.55.0
package main

import (
	"bytes"
	"compress/gzip"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

// repo is GitHub's current name for this repo (`gh repo view`), set in `main`. Not a constant, so
// the move to the `getcmdr` org (#476) needed no change here. A release built before the move still
// verifies under the name it was built under (`builtin.go`).
var repo string

const (
	manifestURL      = "https://getcmdr.com/latest.json"
	pipelineWorkflow = ".github/workflows/release-pipeline.yml"
	sbomPredicate    = "https://cyclonedx.org/bom"
)

var versionRE = regexp.MustCompile(`^\d+\.\d+\.\d+$`)

type config struct {
	root          string
	version       string
	tag           string
	withPkg       bool
	manifestWait  time.Duration
	outDir        string
	archive       string
	noUpload      bool
	oldMacOSSmoke bool
}

func main() {
	root := flag.String("root", "", "the repo root (scripts/release-evidence.sh sets it)")
	flag.Usage = usage
	flag.Parse()
	if *root == "" || flag.NArg() < 1 {
		usage()
		os.Exit(2)
	}
	sub := flag.Arg(0)
	fs := flag.NewFlagSet(sub, flag.ExitOnError)
	cfg := config{root: *root}
	noPkg := fs.Bool("no-pkg", false, "accept a release without the installer package (the pkg job skips when the installer certificate is missing)")
	fs.DurationVar(&cfg.manifestWait, "manifest-wait", 2*time.Minute, "how long to wait for getcmdr.com/latest.json to name the version")
	if sub == "evidence" {
		fs.StringVar(&cfg.outDir, "out", "", "where to collect (into OUT/vX.Y.Z; default: a fresh temp dir)")
		fs.StringVar(&cfg.archive, "archive", os.Getenv(archiveEnv), "the archive folder to upload into, as //host/share/path (default: $"+archiveEnv+")")
		fs.BoolVar(&cfg.noUpload, "no-upload", false, "collect only; upload nothing")
		fs.BoolVar(&cfg.oldMacOSSmoke, "old-macos-smoke", false, "the old-macOS smoke test ran for this release (docs/guides/releasing.md § Pre-release smoke test on old macOS)")
	} else if sub != "verify" {
		usage()
		os.Exit(2)
	}
	_ = fs.Parse(flag.Args()[1:])
	if fs.NArg() != 1 || !versionRE.MatchString(fs.Arg(0)) {
		fmt.Fprintln(os.Stderr, "Give one release version, like 0.55.0.")
		os.Exit(2)
	}
	cfg.version, cfg.tag, cfg.withPkg = fs.Arg(0), "v"+fs.Arg(0), !*noPkg

	name, err := output(cfg.root, "gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner")
	if err != nil {
		fail(fmt.Errorf("couldn't ask GitHub for this repo's name: %w", err))
	}
	repo = name
	if sub == "verify" {
		err = runVerify(cfg)
	} else {
		err = runEvidence(cfg)
	}
	if err != nil {
		fail(err)
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "Usage: ./scripts/release-evidence.sh verify [-no-pkg] [-manifest-wait D] <version>")
	fmt.Fprintln(os.Stderr, "       ./scripts/release-evidence.sh evidence [-archive //host/share/path | -no-upload] [-out DIR] [-old-macos-smoke] [-no-pkg] <version>")
}

func fail(err error) {
	fmt.Fprintf(os.Stderr, "\n✗ %v\n", err)
	os.Exit(1)
}

func step(format string, args ...any) {
	fmt.Printf("==> "+format+"\n", args...)
}

func ok(format string, args ...any) {
	fmt.Printf("  ✓ "+format+"\n", args...)
}

func runVerify(cfg config) error {
	work, err := os.MkdirTemp("", "cmdr-release-verify-")
	if err != nil {
		return err
	}
	defer func() { _ = os.RemoveAll(work) }()
	_, err = verify(cfg, work)
	if err == nil {
		fmt.Printf("\n✓ %s's public surface checks out.\n", cfg.tag)
	}
	return err
}

// verified is what `verify` learned on the way, which `evidence` builds on.
type verified struct {
	taggedCommit string
	ci           ciCandidate
	ciRun        workflowRun
	codeQLRun    workflowRun
	releaseRuns  []workflowRun
	attestation  string // the `gh attestation verify` record for attestation-verify.txt
}

// verify runs every public-surface check, using work for the DMG it downloads.
func verify(cfg config, work string) (verified, error) {
	var v verified
	if err := verifyRelease(cfg); err != nil {
		return v, err
	}
	built, err := releaseBuiltIn(cfg, work)
	if err != nil {
		return v, err
	}
	if err := verifyManifest(cfg, built); err != nil {
		return v, err
	}
	tagged, parent, err := taggedCommit(cfg.tag)
	if err != nil {
		return v, err
	}
	v.taggedCommit = tagged
	if v.releaseRuns, err = verifyReleaseRuns(cfg.tag, tagged); err != nil {
		return v, err
	}
	if v.ci, v.ciRun, v.codeQLRun, err = findCIRuns(tagged, parent); err != nil {
		return v, err
	}
	v.attestation, err = verifyAttestations(cfg, built, tagged, work)
	return v, err
}

// releaseBuiltIn reads which repo built the release (`builtInRepo`) from the `latest.json` on it.
func releaseBuiltIn(cfg config, work string) (string, error) {
	if _, err := output("", "gh", "release", "download", cfg.tag, "-R", repo, "-p", "latest.json", "-D", work, "--clobber"); err != nil {
		return "", err
	}
	body, err := os.ReadFile(filepath.Join(work, "latest.json"))
	if err != nil {
		return "", err
	}
	built, err := builtInRepo(body, cfg.version, repo)
	if err != nil {
		return "", err
	}
	if built != repo {
		ok("built under %s, before the repo moved: its attestations and update URLs name that", built)
	}
	return built, nil
}

func verifyRelease(cfg config) error {
	step("Checking the GitHub release %s", cfg.tag)
	out, err := output("", "gh", "release", "view", cfg.tag, "-R", repo, "--json", "isDraft,assets")
	if err != nil {
		return err
	}
	var rel struct {
		IsDraft bool           `json:"isDraft"`
		Assets  []releaseAsset `json:"assets"`
	}
	if err := json.Unmarshal([]byte(out), &rel); err != nil {
		return fmt.Errorf("`gh release view` didn't print JSON: %w", err)
	}
	if rel.IsDraft {
		return fmt.Errorf("%s is still a draft. In `local` mode, `release-finish.sh` publishes it", cfg.tag)
	}
	if err := checkAssets(rel.Assets, cfg.version, cfg.withPkg); err != nil {
		return err
	}
	for _, a := range rel.Assets {
		if strings.HasSuffix(a.Name, ".dmg") || strings.HasSuffix(a.Name, ".pkg") {
			ok("%s (%s bytes)", a.Name, groupThousands(a.Size))
		}
	}
	ok("all %d expected assets are there", len(expectedAssets(cfg.version, cfg.withPkg)))
	return nil
}

var httpClient = &http.Client{Timeout: 60 * time.Second}

// verifyManifest checks getcmdr.com/latest.json, whose URLs name the repo that built the release.
func verifyManifest(cfg config, built string) error {
	step("Checking %s", manifestURL)
	deadline := time.Now().Add(cfg.manifestWait)
	var urls []string
	for {
		body, err := fetch(manifestURL)
		if err == nil {
			urls, err = checkManifest(body, cfg.version, built)
		}
		if err == nil {
			break
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("%w. If the version is stale, the website deploy may have failed silently: re-run the website-deploy workflow (`workflow_dispatch`)", err)
		}
		fmt.Printf("    not yet (%v); retrying in 15 s\n", err)
		time.Sleep(15 * time.Second)
	}
	ok("latest.json says %s", cfg.version)
	for _, u := range urls {
		if err := probe(u); err != nil {
			return err
		}
		ok("200 %s", u)
	}
	return nil
}

func fetch(url string) ([]byte, error) {
	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Cache-Control", "no-cache")
	resp, err := httpClient.Do(req)
	if err != nil || resp == nil {
		return nil, fmt.Errorf("couldn't reach %s: %v", url, err)
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("%s answered %s", url, resp.Status)
	}
	return io.ReadAll(resp.Body)
}

// probe HEADs a URL, following redirects, and requires a 200: what the updater's download sees.
func probe(url string) error {
	resp, err := httpClient.Head(url)
	if err != nil || resp == nil {
		return fmt.Errorf("couldn't reach %s: %v", url, err)
	}
	_ = resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("%s answered %s, so installs can't download this update", url, resp.Status)
	}
	return nil
}

// taggedCommit is the commit the tag names and its first parent, as GitHub has them.
func taggedCommit(tag string) (commit, parent string, err error) {
	out, err := output("", "gh", "api", "repos/"+repo+"/commits/"+tag, "--jq", "[.sha, .parents[0].sha] | join(\" \")")
	if err != nil {
		return "", "", err
	}
	fields := strings.Fields(out)
	if len(fields) != 2 {
		return "", "", fmt.Errorf("couldn't read %s's commit and parent from GitHub (got %q)", tag, out)
	}
	return fields[0], fields[1], nil
}

func listRuns(args ...string) ([]workflowRun, error) {
	out, err := output("", "gh", append([]string{"run", "list", "-R", repo, "--limit", "20",
		"--json", "databaseId,event,status,conclusion,createdAt,headSha"}, args...)...)
	if err != nil {
		return nil, err
	}
	var runs []workflowRun
	if err := json.Unmarshal([]byte(out), &runs); err != nil {
		return nil, fmt.Errorf("`gh run list` didn't print JSON: %w", err)
	}
	return runs, nil
}

func runJobs(id int64) ([]runJob, error) {
	out, err := output("", "gh", "run", "view", fmt.Sprint(id), "-R", repo, "--json", "jobs")
	if err != nil {
		return nil, err
	}
	var v struct {
		Jobs []runJob `json:"jobs"`
	}
	if err := json.Unmarshal([]byte(out), &v); err != nil {
		return nil, fmt.Errorf("`gh run view %d --json jobs` didn't print JSON: %w", id, err)
	}
	return v.Jobs, nil
}

func verifyReleaseRuns(tag, commit string) ([]workflowRun, error) {
	step("Checking the release.yml runs for %s", tag)
	all, err := listRuns("--workflow", "release.yml", "--branch", tag)
	if err != nil {
		return nil, err
	}
	runs, err := pickReleaseRuns(all, commit)
	if err != nil {
		return nil, err
	}
	for _, r := range runs {
		jobs, err := runJobs(r.ID)
		if err != nil {
			return nil, err
		}
		if err := checkAttestJob(r.ID, jobs); err != nil {
			return nil, err
		}
		ok("%d (%s) green, attest job included", r.ID, eventLabel(r.Event))
	}
	return runs, nil
}

func findCIRuns(tagged, parent string) (ciCandidate, workflowRun, workflowRun, error) {
	step("Finding the full CI run the release rests on")
	cands := []ciCandidate{
		{commit: tagged, describe: "the tagged `chore(release)` commit"},
		{commit: parent, describe: "the parent of the `chore(release)` commit"},
	}
	for i := range cands {
		runs, err := listRuns("--workflow", "ci.yml", "--commit", cands[i].commit, "--event", "workflow_dispatch")
		if err != nil {
			return ciCandidate{}, workflowRun{}, workflowRun{}, err
		}
		cands[i].runs = runs
		cands[i].jobs = map[int64][]runJob{}
		for _, r := range runs {
			if cands[i].jobs[r.ID], err = runJobs(r.ID); err != nil {
				return ciCandidate{}, workflowRun{}, workflowRun{}, err
			}
		}
	}
	ci, ciRun, err := pickFullRun(cands)
	if err != nil {
		return ciCandidate{}, workflowRun{}, workflowRun{}, err
	}
	ok("ci.yml %d green on %s, %s", ciRun.ID, short(ci.commit), ci.describe)
	codeQLRuns, err := listRuns("--workflow", "codeql.yml", "--commit", ci.commit)
	if err != nil {
		return ciCandidate{}, workflowRun{}, workflowRun{}, err
	}
	codeQLRun, err := pickCodeQLRun(codeQLRuns, ci.commit)
	if err != nil {
		return ciCandidate{}, workflowRun{}, workflowRun{}, err
	}
	ok("codeql.yml %d green on the same commit", codeQLRun.ID)
	return ci, ciRun, codeQLRun, nil
}

// verifyAttestations runs both checks in § Provenance and SBOM attestations on the Apple
// Silicon DMG, pinned like `release-finish.sh` pins them to the repo that built the release, and
// returns the record to keep.
func verifyAttestations(cfg config, built, commit, work string) (string, error) {
	dmg := "Cmdr_" + cfg.version + "_aarch64.dmg"
	step("Verifying %s's attestations", dmg)
	if _, err := output("", "gh", "release", "download", cfg.tag, "-R", repo, "-p", dmg, "-D", work, "--clobber"); err != nil {
		return "", err
	}
	pinned := []string{"--repo", built, "--format", "json",
		"--signer-workflow", built + "/" + pipelineWorkflow, "--signer-digest", commit,
		"--source-ref", "refs/tags/" + cfg.tag, "--source-digest", commit, "--deny-self-hosted-runners"}
	checks := []struct {
		title, label string
		extra        []string
	}{
		{"provenance (signer release-pipeline.yml at refs/tags/" + cfg.tag + ")", "provenance", nil},
		{"SBOM binding", "SBOM binding", []string{"--predicate-type", sbomPredicate}},
	}
	var record strings.Builder
	for _, c := range checks {
		args := append(append([]string{"attestation", "verify", filepath.Join(work, dmg)}, pinned...), c.extra...)
		out, err := output("", "gh", args...)
		if err != nil {
			return "", fmt.Errorf("the %s check failed on %s. Read docs/guides/releasing.md § The attest or sbom job failed.\n%w", c.label, dmg, err)
		}
		summary, err := summarizeAttestations([]byte(out))
		if err != nil {
			return "", err
		}
		fmt.Fprintf(&record, "== %s\n$ gh %s\nexit 0\n%s\n\n", c.title, strings.Join(append(append([]string{"attestation", "verify", dmg}, pinned...), c.extra...), " "), summary)
		ok("%s verified", c.label)
	}
	return record.String(), nil
}

func runEvidence(cfg config) error {
	var folder string
	if !cfg.noUpload {
		if cfg.archive == "" {
			return fmt.Errorf("give the archive folder with -archive or $%s (the CRA release log note in the vault has it), or pass -no-upload", archiveEnv)
		}
		var err error
		if folder, err = archiveFolder(cfg.archive, cfg.tag); err != nil {
			return err
		}
	}
	dir, err := evidenceDir(cfg)
	if err != nil {
		return err
	}
	work, err := os.MkdirTemp("", "cmdr-release-verify-")
	if err != nil {
		return err
	}
	defer func() { _ = os.RemoveAll(work) }()
	v, err := verify(cfg, work)
	if err != nil {
		return err
	}
	facts, err := collect(cfg, v, dir)
	if err != nil {
		return err
	}
	if folder != "" {
		step("Uploading to the archive")
		if facts.archiveFiles, err = upload(dir, folder); err != nil {
			return err
		}
		facts.archivePath = pathInShare(folder)
		ok("%d files listed back in %s", facts.archiveFiles, facts.archivePath)
	}
	fmt.Printf("\n✓ Evidence for %s collected in %s\n", cfg.tag, dir)
	printLanes(facts.lanes)
	fmt.Printf("\nRelease log sub-bullets (paste under the %s line, then add any judgment, like what the security section covers):\n\n%s", cfg.tag, releaseLogBullets(facts))
	return nil
}

func printLanes(lanes []testLane) {
	fmt.Println("\nTest lanes in the CI log:")
	for _, l := range lanes {
		fmt.Printf("  %-4s %s / %s: %s\n", l.status, l.job, l.name, withThousands(l.summary))
	}
}

// evidenceDir makes the empty `vX.Y.Z` folder to collect into.
func evidenceDir(cfg config) (string, error) {
	base := cfg.outDir
	if base == "" {
		var err error
		if base, err = os.MkdirTemp("", "cmdr-release-evidence-"); err != nil {
			return "", err
		}
	}
	dir := filepath.Join(base, cfg.tag)
	if entries, err := os.ReadDir(dir); err == nil && len(entries) > 0 {
		return "", fmt.Errorf("%s isn't empty; give a fresh -out", dir)
	}
	return dir, os.MkdirAll(dir, 0o755)
}

// collect writes the evidence files into dir and gathers what the sub-bullets say.
func collect(cfg config, v verified, dir string) (releaseFacts, error) {
	f := releaseFacts{repo: repo, version: cfg.version, tag: cfg.tag, taggedCommit: v.taggedCommit,
		ci: v.ci, ciRun: v.ciRun, codeQLRun: v.codeQLRun, releaseRuns: v.releaseRuns, oldMacOSSmoke: cfg.oldMacOSSmoke}
	step("Collecting the evidence into %s", dir)
	if _, err := output("", "gh", "release", "download", cfg.tag, "-R", repo, "-D", dir,
		"-p", "*.cdx.json", "-p", "*.intoto.jsonl", "-p", "checksums.txt", "-p", "latest.json"); err != nil {
		return f, err
	}
	ok("release assets: SBOMs, provenance bundle, checksums.txt, latest.json")
	if err := os.WriteFile(filepath.Join(dir, "attestation-verify.txt"), []byte(v.attestation), 0o644); err != nil {
		return f, err
	}
	if err := saveRun("ci", v.ciRun.ID, dir); err != nil {
		return f, err
	}
	for _, r := range v.releaseRuns {
		if err := saveRun("release", r.ID, dir); err != nil {
			return f, err
		}
	}
	if err := saveCodeQL(v.ci.commit, dir); err != nil {
		return f, err
	}
	lanes, err := readLanes(filepath.Join(dir, fmt.Sprintf("ci-%d-log.txt.gz", v.ciRun.ID)))
	if err != nil {
		return f, err
	}
	f.lanes = lanes
	changelog, err := output("", "gh", "api", "-H", "Accept: application/vnd.github.raw", "repos/"+repo+"/contents/CHANGELOG.md?ref="+cfg.tag)
	if err != nil {
		return f, err
	}
	f.hasSecurity, f.advisoryIDs, err = securityContent(changelog, cfg.version)
	return f, err
}

// saveRun keeps a run's jobs JSON and its full log, gzipped (a full CI log is ~3 MB, ~350 KB
// gzipped). GitHub drops both after 90 days.
func saveRun(workflow string, id int64, dir string) error {
	jobs, err := output("", "gh", "run", "view", fmt.Sprint(id), "-R", repo, "--json", "jobs")
	if err != nil {
		return err
	}
	base := filepath.Join(dir, fmt.Sprintf("%s-%d", workflow, id))
	if err := os.WriteFile(base+"-jobs.json", []byte(jobs+"\n"), 0o644); err != nil {
		return err
	}
	log, err := os.Create(base + "-log.txt.gz")
	if err != nil {
		return err
	}
	zw := gzip.NewWriter(log)
	jobIDs, err := loggedJobIDs([]byte(jobs))
	runErr := err
	if runErr == nil {
		runErr = writeJobLogs(jobIDs, zw)
	}
	closeErr := errors.Join(zw.Close(), log.Close())
	if runErr != nil {
		return fmt.Errorf("run %d's log: %w", id, runErr)
	}
	if closeErr != nil {
		return closeErr
	}
	ok("%s run %d: jobs JSON and gzipped log (%d jobs)", workflow, id, len(jobIDs))
	return nil
}

// loggedJobIDs lists the jobs of a `gh run view --json jobs` answer that have a log, in run
// order. A skipped job never ran, so it has none.
func loggedJobIDs(jobsJSON []byte) ([]int64, error) {
	var run struct {
		Jobs []struct {
			DatabaseID int64  `json:"databaseId"`
			Conclusion string `json:"conclusion"`
		} `json:"jobs"`
	}
	if err := json.Unmarshal(jobsJSON, &run); err != nil {
		return nil, fmt.Errorf("reading the jobs JSON: %w", err)
	}
	var ids []int64
	for _, job := range run.Jobs {
		if job.Conclusion != "skipped" {
			ids = append(ids, job.DatabaseID)
		}
	}
	return ids, nil
}

// writeJobLogs appends each job's log to w. One `gh run view --log` per job, never one for the
// whole run: since CI split its lanes into slices, a full run has 30+ jobs, and gh refuses a
// whole-run log past its API budget ("too many API requests needed to fetch logs", v0.60.0's
// evidence run, 2026-11-06). Each job's lines carry the same `<job>\t<step>\t<timestamp> <text>`
// shape, so `parseTestLanes` reads the joined log unchanged.
func writeJobLogs(jobIDs []int64, w io.Writer) error {
	for _, jobID := range jobIDs {
		cmd := exec.Command("gh", "run", "view", "--job", fmt.Sprint(jobID), "-R", repo, "--log")
		var stderr bytes.Buffer
		cmd.Stdout, cmd.Stderr = w, &stderr
		if err := cmd.Run(); err != nil {
			return fmt.Errorf("gh run view --job %d --log: %w\n%s", jobID, err, strings.TrimSpace(stderr.String()))
		}
	}
	return nil
}

func saveCodeQL(commit, dir string) error {
	list, err := output("", "gh", "api", "repos/"+repo+"/code-scanning/analyses?tool_name=CodeQL&per_page=100")
	if err != nil {
		return err
	}
	analyses, err := pickAnalyses([]byte(list), commit)
	if err != nil {
		return err
	}
	for _, a := range analyses {
		sarif, err := output("", "gh", "api", "-H", "Accept: application/sarif+json", fmt.Sprintf("repos/%s/code-scanning/analyses/%d", repo, a.ID))
		if err != nil {
			return err
		}
		if err := os.WriteFile(filepath.Join(dir, fmt.Sprintf("codeql-%d.sarif", a.ID)), []byte(sarif+"\n"), 0o644); err != nil {
			return err
		}
		ok("CodeQL SARIF %d (%s)", a.ID, a.Category)
	}
	return nil
}

func readLanes(gzPath string) ([]testLane, error) {
	file, err := os.Open(gzPath)
	if err != nil {
		return nil, err
	}
	defer func() { _ = file.Close() }()
	zr, err := gzip.NewReader(file)
	if err != nil {
		return nil, err
	}
	return parseTestLanes(zr)
}

// output runs a command and returns its trimmed stdout, with stderr in the error.
func output(dir string, name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("%s %s: %w\n%s", name, strings.Join(args, " "), err, strings.TrimSpace(stderr.String()))
	}
	return strings.TrimSpace(string(out)), nil
}
