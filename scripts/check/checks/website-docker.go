package checks

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// RunWebsiteDockerBuild runs the full Docker build for the website.
// Catches .dockerignore mismatches, missing COPY sources, pnpm install failures,
// and Astro build errors in the container context, anything that would break the deploy.
// It then starts the built image and probes the nginx behavior a build alone can't show
// (`probeWebsiteContainer`).
//
// The deploy copies the browser demo from its published image; here it comes from a local
// `apps/desktop/build-demo/` when one is built (`pnpm build:demo`), else from an empty dir with
// the demo off, so the check never needs the registry or a Rust build.
// websiteDemoImagePlaceholder is the image the Dockerfile's `demo-bundle` stage names when no
// `DEMO_IMAGE_DIGEST` is passed (its placeholder default), so a named build context can stand in.
const websiteDemoImagePlaceholder = "ghcr.io/vdavid/cmdr-demo@sha256:0000000000000000000000000000000000000000000000000000000000000000"

func RunWebsiteDockerBuild(ctx *CheckContext) (CheckResult, error) {
	if !CommandExists("docker") {
		return SkippedNoEnvironment("docker not installed"), nil
	}

	args := []string{"build", "-f", "apps/website/Dockerfile", "-t", "getcmdr-check"}
	demoDir := filepath.Join(ctx.RootDir, "apps", "desktop", "build-demo")
	_, statErr := os.Stat(filepath.Join(demoDir, "index.html"))
	withDemo := statErr == nil
	if withDemo {
		args = append(args, "--build-context", websiteDemoImagePlaceholder+"="+demoDir)
	} else {
		emptyDir, err := os.MkdirTemp("", "website-docker-no-demo-")
		if err != nil {
			return CheckResult{}, err
		}
		defer os.RemoveAll(emptyDir)
		args = append(args, "--build-context", websiteDemoImagePlaceholder+"="+emptyDir, "--build-arg", "CMDR_WEBSITE_DEMO=0")
	}

	cmd := exec.Command("docker", append(args, ".")...)
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("docker build failed\n%s", indentOutput(output))
	}

	probes, err := probeWebsiteContainer("getcmdr-check", withDemo)
	if err != nil {
		return CheckResult{}, err
	}
	return Success(fmt.Sprintf("Docker build passed, %d serving probes passed", probes)), nil
}

// websiteServingProbe is one request against the running image and what its answer must look like.
type websiteServingProbe struct {
	path       string
	wantStatus int
	// wantEmptyBody: the answer carries no body at all (a bare status).
	wantEmptyBody bool
	// wantHeaders: each header must contain its value (case-sensitive substring).
	wantHeaders map[string]string
	// wantNoHeaders: each header must be absent.
	wantNoHeaders []string
	// needsDemo: the probe only means something when the image holds a demo build, so it runs only then.
	needsDemo bool
}

// websiteServingProbes are the nginx behaviors a build can't prove.
//
// A missing file under `/demo/_app/immutable/` (a stale chunk name in an old tab after a deploy)
// must be a bare 404 under the demo's headers, never the site's HTML 404 page under the site's
// CSP: the demo's loader can then tell a gone chunk from a real failure (issue #961).
//
// Search engines may list `/demo/` itself, so it goes out without `X-Robots-Tag`, while the app's
// own routes under it (its secondary windows, served the same page by the SPA fallback) stay
// `noindex` (`$demo_robots` in nginx.conf).
var websiteServingProbes = []websiteServingProbe{
	{
		path:          "/demo/",
		wantStatus:    http.StatusOK,
		wantNoHeaders: []string{"X-Robots-Tag"},
		needsDemo:     true,
	},
	{
		path:          "/demo/?free=1000",
		wantStatus:    http.StatusOK,
		wantNoHeaders: []string{"X-Robots-Tag"},
		needsDemo:     true,
	},
	{
		path:        "/demo/settings",
		wantStatus:  http.StatusOK,
		wantHeaders: map[string]string{"X-Robots-Tag": "noindex"},
		needsDemo:   true,
	},
	{
		path:          "/demo/_app/immutable/chunks/missing-check-probe.js",
		wantStatus:    http.StatusNotFound,
		wantEmptyBody: true,
		wantHeaders: map[string]string{
			"Content-Security-Policy": "'wasm-unsafe-eval'",
			"X-Robots-Tag":            "noindex",
		},
	},
	{
		path:          "/demo/_app/immutable/assets/missing-check-probe.png",
		wantStatus:    http.StatusNotFound,
		wantEmptyBody: true,
		wantHeaders: map[string]string{
			"Content-Security-Policy": "'wasm-unsafe-eval'",
			"X-Robots-Tag":            "noindex",
		},
	},
}

// probeWebsiteContainer starts the image on a random loopback port, runs every probe, and stops
// it. `withDemo`: the image holds a demo build, so the probes that need one run too. Returns how
// many probes passed, or an error listing every one that didn't.
func probeWebsiteContainer(image string, withDemo bool) (int, error) {
	runOut, err := exec.Command("docker", "run", "-d", "--rm", "-p", "127.0.0.1::80", image).Output()
	if err != nil {
		return 0, fmt.Errorf("docker run of the built image failed: %w", err)
	}
	containerID := strings.TrimSpace(string(runOut))
	defer func() { _ = exec.Command("docker", "rm", "-f", containerID).Run() }()

	portOut, err := exec.Command("docker", "port", containerID, "80/tcp").Output()
	if err != nil {
		return 0, fmt.Errorf("docker port failed: %w", err)
	}
	// `docker port` prints one `127.0.0.1:<port>` line per binding.
	firstLine, _, _ := strings.Cut(string(portOut), "\n")
	hostPort := strings.TrimSpace(firstLine)
	baseURL := "http://" + hostPort
	client := &http.Client{Timeout: 5 * time.Second}

	// nginx needs a moment to start listening.
	deadline := time.Now().Add(20 * time.Second)
	for {
		resp, err := client.Get(baseURL + "/")
		if err == nil && resp != nil {
			_ = resp.Body.Close()
			break
		}
		if time.Now().After(deadline) {
			return 0, fmt.Errorf("the built image never answered on %s: %w", baseURL, err)
		}
		time.Sleep(200 * time.Millisecond)
	}

	var failures []string
	ran := 0
	for _, probe := range websiteServingProbes {
		if probe.needsDemo && !withDemo {
			continue
		}
		ran++
		failures = append(failures, runWebsiteServingProbe(client, baseURL, probe)...)
	}
	if len(failures) > 0 {
		return 0, fmt.Errorf("the built image serves wrong\n%s", indentOutput(strings.Join(failures, "\n")))
	}
	return ran, nil
}

func runWebsiteServingProbe(client *http.Client, baseURL string, probe websiteServingProbe) []string {
	resp, err := client.Get(baseURL + probe.path)
	if err != nil || resp == nil {
		return []string{fmt.Sprintf("%s: request failed: %v", probe.path, err)}
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)

	var failures []string
	if resp.StatusCode != probe.wantStatus {
		failures = append(failures, fmt.Sprintf("%s: status %d, want %d", probe.path, resp.StatusCode, probe.wantStatus))
	}
	if probe.wantEmptyBody && len(body) > 0 {
		failures = append(failures, fmt.Sprintf("%s: %d-byte body (%s), want none", probe.path, len(body), resp.Header.Get("Content-Type")))
	}
	for name, want := range probe.wantHeaders {
		if got := resp.Header.Get(name); !strings.Contains(got, want) {
			failures = append(failures, fmt.Sprintf("%s: header %s is %q, want it to contain %q", probe.path, name, got, want))
		}
	}
	for _, name := range probe.wantNoHeaders {
		if got := resp.Header.Get(name); got != "" {
			failures = append(failures, fmt.Sprintf("%s: header %s is %q, want none", probe.path, name, got))
		}
	}
	return failures
}
