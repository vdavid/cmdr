package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// RunWebsiteDockerBuild runs the full Docker build for the website.
// Catches .dockerignore mismatches, missing COPY sources, pnpm install failures,
// and Astro build errors in the container context, anything that would break the deploy.
//
// The deploy copies the browser demo from its published image; here it comes from a local
// `apps/desktop/build-demo/` when one is built (`pnpm build:demo`), else from an empty dir with
// the demo off, so the check never needs the registry or a Rust build.
func RunWebsiteDockerBuild(ctx *CheckContext) (CheckResult, error) {
	if !CommandExists("docker") {
		return Skipped("docker not installed"), nil
	}

	args := []string{"build", "-f", "apps/website/Dockerfile", "-t", "getcmdr-check"}
	demoDir := filepath.Join(ctx.RootDir, "apps", "desktop", "build-demo")
	if _, err := os.Stat(filepath.Join(demoDir, "index.html")); err == nil {
		args = append(args, "--build-context", "demo="+demoDir)
	} else {
		emptyDir, err := os.MkdirTemp("", "website-docker-no-demo-")
		if err != nil {
			return CheckResult{}, err
		}
		defer os.RemoveAll(emptyDir)
		args = append(args, "--build-context", "demo="+emptyDir, "--build-arg", "CMDR_WEBSITE_DEMO=0")
	}

	cmd := exec.Command("docker", append(args, ".")...)
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("docker build failed\n%s", indentOutput(output))
	}
	return Success("Docker build passed"), nil
}
