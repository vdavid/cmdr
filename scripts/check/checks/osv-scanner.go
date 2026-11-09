package checks

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"os/exec"
	"path/filepath"
	"slices"
	"sort"
	"strconv"
	"strings"
	"time"
)

// RunOSVScanner scans every lockfile git tracks (Rust, npm, Go, Python) against the OSV database.
// OpenSSF Scorecard's Vulnerabilities check runs osv-scanner's v2 library over the same files,
// honoring each directory's `osv-scanner.toml`, so running the same tool the same way is what
// keeps that score honest. Accepted advisories live in the `osv-scanner.toml` beside each
// lockfile, each with its reason.
//
// It shares govulncheck's and cargo-deny's fix-age policy (`vuln-fix-age.go`): a new advisory warns while its fix
// is under six days old, then fails. A warning is never cached and a pass is trusted for a day
// (`advisoryCacheTTL`), so a fresh advisory surfaces on the next daily run without failing CI
// the morning it's published.
func RunOSVScanner(ctx *CheckContext) (CheckResult, error) {
	scanner, err := EnsureGoTool(ctx.RootDir, "osv-scanner", "github.com/google/osv-scanner/v2/cmd/osv-scanner@v2.6.0")
	if err != nil {
		return CheckResult{}, err
	}
	tree, cleanup, err := exportTrackedTree(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	defer cleanup()

	// Exit 0 is clean and 1 is "found something"; anything else (128: no lockfiles, 127: a
	// scanner error) means the scan didn't happen.
	cmd := exec.Command(scanner, "scan", "source", "--recursive", "--format", "json", tree)
	stdout, err := cmd.Output()
	var exitErr *exec.ExitError
	if err != nil && (!errors.As(err, &exitErr) || exitErr.ExitCode() != 1) {
		return CheckResult{}, fmt.Errorf("osv-scanner couldn't scan the repo: %w\n%s", err, indentOutput(string(stderrOf(err))))
	}

	findings, err := parseOSVReport(stdout, tree)
	if err != nil {
		return CheckResult{}, err
	}

	// One finding at a time, so each verdict keeps its lockfile: the same advisory often sits in
	// several (the fuzz harness and the benchmark carry their own copies of shared crates).
	releases := newRegistryReleaseTimes()
	now := time.Now()
	var blockingLines []string
	pending := map[vulnPendingFix]*vulnPendingGroup{}
	for _, f := range findings {
		blocking, waiting := splitByFixAge([]vulnAdvisory{f.advisory}, now, releases.fixReleased)
		for _, a := range blocking {
			blockingLines = append(blockingLines, fmt.Sprintf("[%s] %s", f.where, a.describe()))
		}
		for _, a := range waiting {
			addPendingFix(pending, a, f.where)
		}
	}
	sort.Strings(blockingLines)
	return vulnVerdict("osv-scanner", blockingLines,
		"Upgrade past the fix, or, if it can't reach what we ship, ignore it with a reason in the osv-scanner.toml beside that lockfile.",
		pending, "No known vulnerabilities in any tracked lockfile")
}

func stderrOf(err error) []byte {
	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) {
		return exitErr.Stderr
	}
	return nil
}

type osvReport struct {
	Results []struct {
		Source struct {
			Path string `json:"path"`
		} `json:"source"`
		Packages []osvPackage `json:"packages"`
	} `json:"results"`
}

type osvPackage struct {
	Package struct {
		Name      string `json:"name"`
		Version   string `json:"version"`
		Ecosystem string `json:"ecosystem"`
	} `json:"package"`
	Groups []struct {
		IDs []string `json:"ids"`
	} `json:"groups"`
	Vulnerabilities []osvVulnerability `json:"vulnerabilities"`
}

type osvVulnerability struct {
	ID        string    `json:"id"`
	Published time.Time `json:"published"`
	Summary   string    `json:"summary"`
	Affected  []struct {
		Package struct {
			Ecosystem string `json:"ecosystem"`
			Name      string `json:"name"`
		} `json:"package"`
		Ranges []osvRange `json:"ranges"`
	} `json:"affected"`
}

type osvEvent struct {
	Introduced   string `json:"introduced"`
	Fixed        string `json:"fixed"`
	LastAffected string `json:"last_affected"`
}

type osvRange struct {
	Type   string     `json:"type"`
	Events []osvEvent `json:"events"`
}

// osvFinding is one advisory group against one package, and the lockfile it came from.
type osvFinding struct {
	advisory vulnAdvisory
	where    string
}

// parseOSVReport reads `osv-scanner --format json`. One finding per GROUP: osv-scanner groups
// aliases (a RUSTSEC and its GHSA twin), and counting both would double every crate advisory.
func parseOSVReport(raw []byte, scannedRoot string) ([]osvFinding, error) {
	var report osvReport
	if err := json.Unmarshal(raw, &report); err != nil {
		return nil, fmt.Errorf("couldn't read osv-scanner's report: %w", err)
	}
	var findings []osvFinding
	for _, result := range report.Results {
		where, err := filepath.Rel(scannedRoot, result.Source.Path)
		if err != nil {
			where = result.Source.Path
		}
		for _, pkg := range result.Packages {
			for _, group := range pkg.Groups {
				if len(group.IDs) > 0 {
					findings = append(findings, osvFinding{advisory: osvGroupAdvisory(pkg, group.IDs), where: where})
				}
			}
		}
	}
	return findings, nil
}

// osvGroupAdvisory folds one alias group into an advisory: the first ID names it, the earliest
// `published` dates it, and the lowest fix any member names for our version is the one to take.
func osvGroupAdvisory(pkg osvPackage, ids []string) vulnAdvisory {
	a := vulnAdvisory{
		ID: ids[0], Ecosystem: pkg.Package.Ecosystem, Module: pkg.Package.Name, Version: pkg.Package.Version,
	}
	for _, v := range pkg.Vulnerabilities {
		if !slices.Contains(ids, v.ID) {
			continue
		}
		if a.Summary == "" {
			a.Summary = v.Summary
		}
		if a.Published.IsZero() || (!v.Published.IsZero() && v.Published.Before(a.Published)) {
			a.Published = v.Published
		}
		for _, aff := range v.Affected {
			if aff.Package.Ecosystem != a.Ecosystem || !strings.EqualFold(aff.Package.Name, a.Module) {
				continue
			}
			if fixed := osvFixedVersion(aff.Ranges, a.Version); fixed != "" &&
				(a.FixedVersion == "" || compareLooseVersions(fixed, a.FixedVersion) < 0) {
				a.FixedVersion = fixed
			}
		}
	}
	return a
}

// osvFixFor fills in a's fix and publish date from its OSV record, read the same way an
// osv-scanner finding is, so cargo-deny and osv-scanner agree on which upgrade clears it. a keeps
// its own summary.
func osvFixFor(a vulnAdvisory, v osvVulnerability) vulnAdvisory {
	var pkg osvPackage
	pkg.Package.Name, pkg.Package.Version, pkg.Package.Ecosystem = a.Module, a.Version, a.Ecosystem
	pkg.Vulnerabilities = []osvVulnerability{v}
	filled := osvGroupAdvisory(pkg, []string{a.ID})
	a.FixedVersion, a.Published = filled.FixedVersion, filled.Published
	return a
}

// osvFix looks a's ID up in the OSV database and fills in its fix (`osvFixFor`). A record it
// can't fetch leaves a without one, which fails it: waiting is only for a fix that provably exists.
func (r *registryReleaseTimes) osvFix(a vulnAdvisory) vulnAdvisory {
	var v osvVulnerability
	if !r.getJSON("https://api.osv.dev/v1/vulns/"+url.PathEscape(a.ID), &v) || v.ID != a.ID {
		return a
	}
	return osvFixFor(a, v)
}

// osvFixedVersion is the fix for the affected range `version` falls in, or "" when no range
// holds it or that range has no fix. GIT ranges name commits, so they're skipped.
func osvFixedVersion(ranges []osvRange, version string) string {
	for _, r := range ranges {
		if r.Type != "SEMVER" && r.Type != "ECOSYSTEM" {
			continue
		}
		introduced := ""
		for _, e := range r.Events {
			switch {
			case e.Introduced != "":
				introduced = e.Introduced
			case e.Fixed != "" && introduced != "":
				if compareLooseVersions(introduced, version) <= 0 && compareLooseVersions(version, e.Fixed) < 0 {
					return e.Fixed
				}
				introduced = ""
			}
		}
	}
	return ""
}

// compareLooseVersions orders dotted versions the way crates, npm, Go modules, and most PyPI
// releases spell them: a leading `v` and `+build` metadata are ignored, missing parts count as
// zero, and a `-prerelease` sorts before its release. Not a full PEP 440 or semver
// implementation; it only has to place a version against an advisory's range bounds.
func compareLooseVersions(a, b string) int {
	coreA, preA := splitLooseVersion(a)
	coreB, preB := splitLooseVersion(b)
	for i := 0; i < len(coreA) || i < len(coreB); i++ {
		partA, partB := partAt(coreA, i), partAt(coreB, i)
		if c := comparePart(partA, partB); c != 0 {
			return c
		}
	}
	switch {
	case preA == preB:
		return 0
	case preA == "":
		return 1
	case preB == "":
		return -1
	case preA < preB:
		return -1
	default:
		return 1
	}
}

func splitLooseVersion(v string) (core []string, pre string) {
	v = strings.TrimPrefix(v, "v")
	if i := strings.IndexByte(v, '+'); i >= 0 {
		v = v[:i]
	}
	if i := strings.IndexByte(v, '-'); i >= 0 {
		v, pre = v[:i], v[i+1:]
	}
	return strings.Split(v, "."), pre
}

func partAt(parts []string, i int) string {
	if i < len(parts) {
		return parts[i]
	}
	return "0"
}

func comparePart(a, b string) int {
	na, errA := strconv.Atoi(a)
	nb, errB := strconv.Atoi(b)
	if errA == nil && errB == nil {
		switch {
		case na < nb:
			return -1
		case na > nb:
			return 1
		}
		return 0
	}
	return strings.Compare(a, b)
}

// registryReleaseTimes dates a fixed version by asking its package registry when it was
// published, once per package@version per run. Go reuses govulncheck's proxy lookup.
type registryReleaseTimes struct {
	client  *http.Client
	goTimes *goProxyReleaseTimes
	cache   map[string]goProxyAnswer
}

func newRegistryReleaseTimes() *registryReleaseTimes {
	return &registryReleaseTimes{
		client:  &http.Client{Timeout: 15 * time.Second},
		goTimes: newGoProxyReleaseTimes(),
		cache:   map[string]goProxyAnswer{},
	}
}

// fixReleased dates an advisory's fix. A Go stdlib fix ships with its advisory, so the advisory's
// own date stands in (`govulnFixReleased` has the evidence). An ecosystem we can't date fails,
// per the policy: waiting is only for a fix that provably exists.
func (r *registryReleaseTimes) fixReleased(a vulnAdvisory) (time.Time, bool) {
	if a.Ecosystem == "Go" {
		version := a.FixedVersion
		if !strings.HasPrefix(version, "v") {
			version = "v" + version
		}
		return govulnFixReleased(vulnAdvisory{Module: a.Module, FixedVersion: version, Published: a.Published}, r.goTimes.lookup)
	}
	key := a.Ecosystem + "\x00" + a.Module + "\x00" + a.FixedVersion
	if answer, hit := r.cache[key]; hit {
		return answer.at, answer.ok
	}
	answer := r.fetch(a.Ecosystem, a.Module, a.FixedVersion)
	r.cache[key] = answer
	return answer.at, answer.ok
}

func (r *registryReleaseTimes) fetch(ecosystem, name, version string) goProxyAnswer {
	switch ecosystem {
	case "crates.io":
		var body struct {
			Version struct {
				CreatedAt time.Time `json:"created_at"`
			} `json:"version"`
		}
		ok := r.getJSON(fmt.Sprintf("https://crates.io/api/v1/crates/%s/%s", url.PathEscape(name), url.PathEscape(version)), &body)
		return goProxyAnswer{at: body.Version.CreatedAt, ok: ok && !body.Version.CreatedAt.IsZero()}
	case "npm":
		var body struct {
			Time map[string]time.Time `json:"time"`
		}
		ok := r.getJSON("https://registry.npmjs.org/"+strings.Replace(url.PathEscape(name), "%40", "@", 1), &body)
		at := body.Time[version]
		return goProxyAnswer{at: at, ok: ok && !at.IsZero()}
	case "PyPI":
		var body struct {
			URLs []struct {
				UploadTime time.Time `json:"upload_time_iso_8601"`
			} `json:"urls"`
		}
		ok := r.getJSON(fmt.Sprintf("https://pypi.org/pypi/%s/%s/json", url.PathEscape(name), url.PathEscape(version)), &body)
		if !ok || len(body.URLs) == 0 {
			return goProxyAnswer{}
		}
		return goProxyAnswer{at: body.URLs[0].UploadTime, ok: !body.URLs[0].UploadTime.IsZero()}
	}
	return goProxyAnswer{}
}

func (r *registryReleaseTimes) getJSON(target string, into any) bool {
	req, err := http.NewRequest(http.MethodGet, target, nil)
	if err != nil {
		return false
	}
	// crates.io refuses requests without a User-Agent that names who's asking.
	req.Header.Set("User-Agent", "cmdr-check-runner (https://github.com/getcmdr/cmdr)")
	resp, err := r.client.Do(req)
	if err != nil || resp == nil {
		return false
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return false
	}
	return json.NewDecoder(resp.Body).Decode(into) == nil
}
