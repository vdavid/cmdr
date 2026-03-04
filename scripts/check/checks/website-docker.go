package checks

import (
	"fmt"
	"os/exec"
)

// RunWebsiteDockerBuild runs the full Docker build for the website.
// Catches .dockerignore mismatches, missing COPY sources, pnpm install failures,
// and Astro build errors in the container context — anything that would break the deploy.
func RunWebsiteDockerBuild(ctx *CheckContext) (CheckResult, error) {
	if !CommandExists("docker") {
		return Skipped("docker not installed"), nil
	}

	cmd := exec.Command("docker", "build", "-f", "apps/website/Dockerfile", "-t", "getcmdr-check", ".")
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("docker build failed\n%s", indentOutput(output))
	}
	return Success("Docker build passed"), nil
}
