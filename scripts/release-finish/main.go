// Release finish: the laptop half of a release whose update archives are signed locally
// (repository variable `RELEASE_UPDATE_SIGNING=local`), so the updater key never touches GitHub.
//
// The tag push builds, notarizes, and attests into a DRAFT release with no updater signatures.
// This then, in order:
//
//  1. waits for that run and requires it green;
//  2. downloads the three `.app.tar.gz` update archives from the draft;
//  3. verifies each one's build provenance with `gh attestation verify`, pinned to
//     `release-pipeline.yml` at the tag's commit on a GitHub-hosted runner, and refuses to
//     sign anything that doesn't verify;
//  4. signs each with the Tauri signer, the key and password read from the sops store
//     (`secret`), never printed and passed only to the signer's environment;
//  5. verifies each signature natively against the app's public key (`minisign.go`);
//  6. uploads the `.sig` files to the draft and checks the uploaded bytes;
//  7. dispatches `release.yml` on the tag, whose `publish` job publishes the draft, ships
//     `latest.json`, deploys the website, and bumps the Homebrew tap, then waits for it.
//
// Every run reads where the release stands on GitHub and picks up from there, so it's safe to
// re-run after a crash, a closed laptop, or a red job: a published release skips straight to
// step 7, and a red finishing run gets its failed jobs re-run once.
//
// # Usage
//
//	./scripts/release-finish.sh 0.51.0
//	./scripts/release-finish.sh -dry-run -out /tmp/sigs 0.51.0
//
// `-verify-dir DIR` only verifies the archives and `.sig` files in DIR against the app's public key,
// with no gh or sops; the `publish` job gates a dispatch on it.
//
// `-dry-run` stops after step 5: it uploads, dispatches, and publishes nothing, and works on an
// already published release. `-signer-workflow` (dry runs only) checks a release from before the
// reusable workflow, whose provenance `release.yml` signed.
//
// The flow, the switch between modes, and recovery: `docs/guides/releasing.md` § Who signs the
// update archives.
package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"time"
)

const (
	repo            = "vdavid/cmdr"
	releaseWorkflow = "release.yml"
	pipelineSigner  = "vdavid/cmdr/.github/workflows/release-pipeline.yml"
	keySecret       = "CMDR_TAURI_SIGNING_PRIVATE_KEY"
	passwordSecret  = "CMDR_TAURI_SIGNING_PRIVATE_KEY_PASSWORD"
	pollEvery       = 30 * time.Second
	// How long a run gets to show up in `gh run list` after the push or dispatch that starts it.
	appearWithin = 3 * time.Minute
)

type config struct {
	root           string
	version        string
	tag            string
	outDir         string
	signerWorkflow string
	dryRun         bool
}

func main() {
	root := flag.String("root", "", "the repo root (scripts/release-finish.sh sets it)")
	dryRun := flag.Bool("dry-run", false, "verify and sign into -out, then stop: upload, dispatch, and publish nothing")
	out := flag.String("out", "", "where to put the downloaded archives and signatures (default: a fresh temp dir)")
	signer := flag.String("signer-workflow", pipelineSigner, "dry runs only: the workflow whose provenance must cover the archives")
	verifyDir := flag.String("verify-dir", "", "only verify the three archives in DIR against their .sig files and the app's public key (the publish job's gate)")
	flag.Usage = func() {
		fmt.Fprintln(os.Stderr, "Usage: ./scripts/release-finish.sh [-dry-run] [-out DIR] [-signer-workflow WORKFLOW] <version>")
		flag.PrintDefaults()
	}
	flag.Parse()
	if flag.NArg() != 1 || *root == "" {
		flag.Usage()
		os.Exit(2)
	}
	cfg := config{
		root:           *root,
		version:        flag.Arg(0),
		tag:            "v" + flag.Arg(0),
		outDir:         *out,
		signerWorkflow: *signer,
		dryRun:         *dryRun,
	}
	if err := checkVersion(cfg.version); err != nil {
		fail(err)
	}
	if *verifyDir != "" {
		if err := verifyOnly(cfg, *verifyDir); err != nil {
			fail(err)
		}
		return
	}
	if cfg.signerWorkflow != pipelineSigner && !cfg.dryRun {
		fail(errors.New("-signer-workflow is for dry runs only: a real release is signed only on release-pipeline.yml's provenance"))
	}
	if err := run(cfg); err != nil {
		fail(err)
	}
}

func fail(err error) {
	fmt.Fprintf(os.Stderr, "\n✗ %v\n", err)
	os.Exit(1)
}

func step(format string, args ...any) {
	fmt.Printf("==> "+format+"\n", args...)
}

func runURL(id int64) string {
	return fmt.Sprintf("https://github.com/%s/actions/runs/%d", repo, id)
}

func run(cfg config) error {
	commit, err := output(cfg.root, nil, "git", "rev-parse", "--verify", "--quiet", cfg.tag+"^{commit}")
	if err != nil {
		return fmt.Errorf("there's no local tag %s. Run this in the clone that tagged the release, or `git fetch origin tag %s` first", cfg.tag, cfg.tag)
	}
	pubkey, err := readPubkey(cfg.root)
	if err != nil {
		return err
	}

	if !cfg.dryRun {
		published, err := waitUntilSignable(cfg)
		if err != nil {
			return err
		}
		if published {
			// Published with no finishing run means its own tag run published it (`ci` mode).
			runs, err := listRuns(cfg.tag, "workflow_dispatch")
			if err != nil {
				return err
			}
			if len(runs) == 0 {
				step("%s was published by its own tag run (signed in CI), so there's nothing to finish.", cfg.tag)
				return nil
			}
			step("%s is already published, so its signatures are live. Only the finishing run is left.", cfg.tag)
			return finish(cfg)
		}
	}
	names := updateArchiveNames(cfg.version)
	if err := requireArchives(cfg, names); err != nil {
		return err
	}

	dir, cleanup, err := workDir(cfg)
	if err != nil {
		return err
	}
	defer cleanup()

	if err := downloadAndVerifyProvenance(cfg, dir, names, commit); err != nil {
		return err
	}
	if err := sign(cfg.root, dir, names); err != nil {
		return err
	}
	if err := verifySignatures(pubkey, dir, names); err != nil {
		return err
	}

	if cfg.dryRun {
		step("Dry run: the archives and their signatures are in %s. Nothing was uploaded.", dir)
		return nil
	}
	if err := uploadSignatures(cfg, dir, names); err != nil {
		return err
	}
	return finish(cfg)
}

// verifyOnly is `-verify-dir`: the `publish` job runs it before publishing a draft, so a
// dispatch can't ship a manifest built from missing, empty, or foreign signatures.
func verifyOnly(cfg config, dir string) error {
	pubkey, err := readPubkey(cfg.root)
	if err != nil {
		return err
	}
	return verifySignatures(pubkey, dir, updateArchiveNames(cfg.version))
}

// waitUntilSignable reports a release that's already published (signing is behind it), and
// otherwise waits for the tag push's run to finish green.
func waitUntilSignable(cfg config) (published bool, err error) {
	rel, found, err := findRelease(cfg.tag)
	if err != nil {
		return false, err
	}
	if found && !rel.IsDraft {
		return true, nil
	}
	return false, waitForBuildRun(cfg.tag)
}

// requireArchives checks the release is in a state to sign: a draft (or, in a dry run, any
// release) carrying all three update archives.
func requireArchives(cfg config, names []string) error {
	rel, found, err := findRelease(cfg.tag)
	if err != nil {
		return err
	}
	if !found {
		return fmt.Errorf("%s has no release on GitHub (a draft included), so there's nothing to sign", cfg.tag)
	}
	if !cfg.dryRun && !rel.IsDraft {
		return fmt.Errorf("%s was published while this waited. Run this again to finish it", cfg.tag)
	}
	if missing := rel.missing(names); len(missing) > 0 {
		return fmt.Errorf("the %s release lacks %s, so the build didn't upload everything", cfg.tag, strings.Join(missing, ", "))
	}
	return nil
}

// workDir is -out, or a temp dir that a real run removes afterwards and a dry run keeps.
func workDir(cfg config) (string, func(), error) {
	noop := func() {}
	if cfg.outDir != "" {
		return cfg.outDir, noop, os.MkdirAll(cfg.outDir, 0o755)
	}
	dir, err := os.MkdirTemp("", "cmdr-release-"+cfg.tag+"-")
	if err != nil || cfg.dryRun {
		return dir, noop, err
	}
	return dir, func() { os.RemoveAll(dir) }, nil
}

func downloadAndVerifyProvenance(cfg config, dir string, names []string, commit string) error {
	step("Downloading the update archives into %s", dir)
	for _, name := range names {
		if _, err := output(cfg.root, nil, "gh", "release", "download", cfg.tag, "-R", repo, "-p", name, "-D", dir, "--clobber"); err != nil {
			return err
		}
	}

	step("Verifying build provenance (%s at %.9s)", cfg.signerWorkflow, commit)
	for _, name := range names {
		if _, err := output(cfg.root, nil, "gh", "attestation", "verify", filepath.Join(dir, name),
			"--repo", repo,
			"--signer-workflow", cfg.signerWorkflow,
			"--signer-digest", commit,
			"--source-ref", "refs/tags/"+cfg.tag,
			"--source-digest", commit,
			"--deny-self-hosted-runners",
		); err != nil {
			return fmt.Errorf("refusing to sign %s: its build provenance didn't verify, so nothing can prove CI built it from %s.\n%w", name, cfg.tag, err)
		}
		fmt.Printf("    %s: built by the release pipeline from %s\n", name, cfg.tag)
	}
	return nil
}

// sign runs the Tauri signer over each archive, writing `<archive>.sig` beside it. The key and
// its password go to the signer's environment only, read fresh from sops.
func sign(root, dir string, names []string) error {
	step("Signing with the updater key from sops")
	key, err := output(root, nil, "secret", keySecret)
	if err != nil {
		return fmt.Errorf("reading %s from sops: %w", keySecret, err)
	}
	password, err := output(root, nil, "secret", passwordSecret)
	if err != nil {
		return fmt.Errorf("reading %s from sops: %w", passwordSecret, err)
	}
	env := []string{"TAURI_SIGNING_PRIVATE_KEY=" + key, "TAURI_SIGNING_PRIVATE_KEY_PASSWORD=" + password}
	for _, name := range names {
		path := filepath.Join(dir, name)
		if err := os.Remove(path + ".sig"); err != nil && !errors.Is(err, os.ErrNotExist) {
			return err
		}
		if _, err := output(filepath.Join(root, "apps", "desktop"), env, "pnpm", "exec", "tauri", "signer", "sign", path); err != nil {
			return fmt.Errorf("signing %s: %w", name, err)
		}
		fmt.Printf("    signed %s\n", name)
	}
	return nil
}

func verifySignatures(pubkey, dir string, names []string) error {
	step("Verifying the signatures against the app's public key")
	for _, name := range names {
		if err := verifyFile(pubkey, filepath.Join(dir, name), filepath.Join(dir, name+".sig")); err != nil {
			return fmt.Errorf("the signature for %s doesn't verify against the app's public key (made with the wrong key?): %w", name, err)
		}
		fmt.Printf("    %s.sig verifies\n", name)
	}
	return nil
}

func uploadSignatures(cfg config, dir string, names []string) error {
	step("Uploading the signatures to the %s draft", cfg.tag)
	args := []string{"release", "upload", cfg.tag, "-R", repo, "--clobber"}
	for _, name := range names {
		args = append(args, filepath.Join(dir, name+".sig"))
	}
	if _, err := output(cfg.root, nil, "gh", args...); err != nil {
		return err
	}
	return checkUploadedSignatures(cfg, dir, names)
}

func verifyFile(pubkey, archivePath, sigPath string) error {
	sig, err := os.ReadFile(sigPath)
	if err != nil {
		return err
	}
	f, err := os.Open(archivePath)
	if err != nil {
		return err
	}
	defer f.Close()
	return verifyTauriSignature(pubkey, string(sig), f)
}

// checkUploadedSignatures downloads the `.sig` assets back and compares them with what was
// signed, so the manifest is built from exactly the signatures verified above.
func checkUploadedSignatures(cfg config, dir string, names []string) error {
	back := filepath.Join(dir, "uploaded")
	if err := os.MkdirAll(back, 0o755); err != nil {
		return err
	}
	for _, name := range names {
		if _, err := output(cfg.root, nil, "gh", "release", "download", cfg.tag, "-R", repo, "-p", name+".sig", "-D", back, "--clobber"); err != nil {
			return err
		}
		local, err := os.ReadFile(filepath.Join(dir, name+".sig"))
		if err != nil {
			return err
		}
		uploaded, err := os.ReadFile(filepath.Join(back, name+".sig"))
		if err != nil {
			return err
		}
		if !bytes.Equal(local, uploaded) {
			return fmt.Errorf("%s.sig on the release differs from the one just signed. Run this again", name)
		}
	}
	fmt.Println("    the release carries the three signatures just verified")
	return nil
}

// finish drives the dispatch run of `release.yml` on the tag until it's green.
func finish(cfg config) error {
	reran := false
	for {
		runs, err := listRuns(cfg.tag, "workflow_dispatch")
		if err != nil {
			return err
		}
		action, r := nextFinishAction(runs)
		switch action {
		case actionDone:
			step("%s is out: the finishing run went green (%s).", cfg.tag, runURL(r.ID))
			fmt.Println("    Next: check getcmdr.com/latest.json, the archive URLs, and the attestations (the release skill's last steps).")
			return nil
		case actionWait:
			if err := waitRun(r.ID); err != nil {
				return err
			}
		case actionDispatch:
			step("Starting the finishing run (%s on %s)", releaseWorkflow, cfg.tag)
			if _, err := output(cfg.root, nil, "gh", "workflow", "run", releaseWorkflow, "-R", repo, "--ref", cfg.tag); err != nil {
				return err
			}
			if err := waitForNewRun(cfg.tag, "workflow_dispatch", runs); err != nil {
				return err
			}
		case actionRerunFailed:
			if reran {
				return fmt.Errorf("the finishing run failed again: %s. Read its failed jobs (docs/guides/releasing.md § Troubleshooting), fix, then run this again", runURL(r.ID))
			}
			step("The finishing run ended %s; re-running its failed jobs (%s)", r.Conclusion, runURL(r.ID))
			if _, err := output(cfg.root, nil, "gh", "run", "rerun", fmt.Sprint(r.ID), "-R", repo, "--failed"); err != nil {
				return err
			}
			reran = true
			if err := waitUntilRestarted(r.ID); err != nil {
				return err
			}
		}
	}
}

// waitForBuildRun waits for the tag push's run and requires it green: the draft, the builds,
// the SBOMs, and the provenance this signs on.
func waitForBuildRun(tag string) error {
	runs, err := listRuns(tag, "push")
	if err != nil {
		return err
	}
	if len(runs) == 0 {
		step("Waiting for the %s tag push to start its release run", tag)
		if err := waitForNewRun(tag, "push", nil); err != nil {
			return fmt.Errorf("%w. Was the tag pushed (`git push origin main --tags`)?", err)
		}
		if runs, err = listRuns(tag, "push"); err != nil {
			return err
		}
	}
	r, _ := newest(runs)
	if !r.completed() {
		step("Waiting for the build run (%s); hosted macOS builds take roughly an hour", runURL(r.ID))
		if err := waitRun(r.ID); err != nil {
			return err
		}
		if runs, err = listRuns(tag, "push"); err != nil {
			return err
		}
		r, _ = newest(runs)
	}
	if !r.succeeded() {
		return fmt.Errorf("the %s build run ended %s: %s. Re-run its failed jobs (`gh run rerun %d --failed`), then run this again", tag, r.Conclusion, runURL(r.ID), r.ID)
	}
	step("The build run is green (%s)", runURL(r.ID))
	return nil
}

func listRuns(tag, event string) ([]workflowRun, error) {
	out, err := output("", nil, "gh", "run", "list", "-R", repo, "-w", releaseWorkflow, "-e", event, "-b", tag,
		"-L", "20", "--json", "databaseId,status,conclusion,createdAt")
	if err != nil {
		return nil, err
	}
	var runs []workflowRun
	if err := json.Unmarshal([]byte(out), &runs); err != nil {
		return nil, fmt.Errorf("reading gh run list: %w", err)
	}
	return runs, nil
}

// waitForNewRun polls until a run shows up that wasn't in `before`.
func waitForNewRun(tag, event string, before []workflowRun) error {
	deadline := time.Now().Add(appearWithin)
	for time.Now().Before(deadline) {
		time.Sleep(5 * time.Second)
		runs, err := listRuns(tag, event)
		if err != nil {
			return err
		}
		for _, r := range runs {
			if !slices.ContainsFunc(before, func(b workflowRun) bool { return b.ID == r.ID }) {
				fmt.Printf("    run %s\n", runURL(r.ID))
				return nil
			}
		}
	}
	return fmt.Errorf("no %s run of %s appeared on %s within %s", event, releaseWorkflow, tag, appearWithin)
}

type runView struct {
	Status     string `json:"status"`
	Conclusion string `json:"conclusion"`
	Jobs       []struct {
		Name       string `json:"name"`
		Status     string `json:"status"`
		Conclusion string `json:"conclusion"`
	} `json:"jobs"`
}

func viewRun(id int64) (runView, error) {
	out, err := output("", nil, "gh", "run", "view", fmt.Sprint(id), "-R", repo, "--json", "status,conclusion,jobs")
	if err != nil {
		return runView{}, err
	}
	var v runView
	if err := json.Unmarshal([]byte(out), &v); err != nil {
		return runView{}, fmt.Errorf("reading gh run view: %w", err)
	}
	return v, nil
}

// waitRun polls a run until it completes, printing each job's status as it changes.
func waitRun(id int64) error {
	seen := map[string]string{}
	for {
		v, err := viewRun(id)
		if err != nil {
			return err
		}
		for _, j := range v.Jobs {
			state := j.Status
			if j.Conclusion != "" {
				state = j.Conclusion
			}
			if seen[j.Name] != state {
				seen[j.Name] = state
				fmt.Printf("    %s: %s\n", j.Name, state)
			}
		}
		if v.Status == "completed" {
			return nil
		}
		time.Sleep(pollEvery)
	}
}

// waitUntilRestarted waits for a re-run to leave `completed`, so the next look doesn't read the
// old attempt's red result and give up.
func waitUntilRestarted(id int64) error {
	deadline := time.Now().Add(appearWithin)
	for time.Now().Before(deadline) {
		v, err := viewRun(id)
		if err != nil {
			return err
		}
		if v.Status != "completed" {
			return nil
		}
		time.Sleep(5 * time.Second)
	}
	return fmt.Errorf("run %s didn't restart within %s", runURL(id), appearWithin)
}

// findRelease looks the tag's release up in the full list, the one place a draft shows with its
// tag (the tags API skips drafts). More than one release on a tag is an error, not a pick.
func findRelease(tag string) (releaseInfo, bool, error) {
	out, err := output("", []string{"TAG=" + tag}, "gh", "api", "--paginate", "repos/"+repo+"/releases?per_page=100",
		"--jq", `.[] | select(.tag_name == env.TAG) | {isDraft: .draft, assets: [.assets[] | {name}]}`)
	if err != nil {
		return releaseInfo{}, false, err
	}
	var found []releaseInfo
	dec := json.NewDecoder(strings.NewReader(out))
	for {
		var r releaseInfo
		if err := dec.Decode(&r); errors.Is(err, io.EOF) {
			break
		} else if err != nil {
			return releaseInfo{}, false, fmt.Errorf("reading the release list: %w", err)
		}
		found = append(found, r)
	}
	switch len(found) {
	case 0:
		return releaseInfo{}, false, nil
	case 1:
		return found[0], true, nil
	default:
		return releaseInfo{}, false, fmt.Errorf("%s has %d releases on GitHub. Delete the extra drafts, then run this again", tag, len(found))
	}
}

// readPubkey reads the updater public key the app is built with.
func readPubkey(root string) (string, error) {
	raw, err := os.ReadFile(filepath.Join(root, "apps", "desktop", "src-tauri", "tauri.conf.json"))
	if err != nil {
		return "", err
	}
	var conf struct {
		Plugins struct {
			Updater struct {
				Pubkey string `json:"pubkey"`
			} `json:"updater"`
		} `json:"plugins"`
	}
	if err := json.Unmarshal(raw, &conf); err != nil {
		return "", fmt.Errorf("reading tauri.conf.json: %w", err)
	}
	if conf.Plugins.Updater.Pubkey == "" {
		return "", errors.New("tauri.conf.json has no plugins.updater.pubkey")
	}
	return conf.Plugins.Updater.Pubkey, nil
}

// output runs a command and returns its trimmed stdout. `env` adds to this process's
// environment for that one command. A failure carries the command's stderr, never its env.
func output(dir string, env []string, name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	if env != nil {
		cmd.Env = append(os.Environ(), env...)
	}
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("%s %s: %w\n%s", name, strings.Join(args, " "), err, strings.TrimSpace(stderr.String()))
	}
	return strings.TrimSpace(string(out)), nil
}
