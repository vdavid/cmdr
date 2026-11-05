package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
)

// nilawayWorkers caps how many per-module `nilaway ./...` processes run at once. Each one
// keeps ~6 cores busy and peaks at up to 3.5 GB, so wall time flattens past two while memory
// keeps climbing. Measurements: checks/DETAILS.md § "nilaway fans out per module".
const nilawayWorkers = 2

// nilawayModule is one Go module the check runs nilaway in.
type nilawayModule struct {
	dir   string // absolute module directory
	label string // repo-relative label printed in findings
}

// nilawayModuleResult is what one module's run produced.
type nilawayModuleResult struct {
	pkgCount int
	issue    string // "" when nilaway passed
}

// RunNilaway detects potential nil pointer dereferences.
//
// The per-module commands are what keeps findings exact (why not one `go.work` run:
// checks/DETAILS.md § "nilaway fans out per module"); only the scheduling is parallel, and
// results are joined in module order, so the output is deterministic.
func RunNilaway(ctx *CheckContext) (CheckResult, error) {
	nilawayBin, err := EnsureGoTool(ctx.RootDir, "nilaway", "go.uber.org/nilaway/cmd/nilaway@v0.0.0-20260808063849-8649a03c818a")
	if err != nil {
		return CheckResult{}, err
	}

	allModules, err := FindAllGoModules(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find Go modules: %w", err)
	}

	results := runPerModule(nilawayModules(ctx.RootDir, allModules), nilawayWorkers, func(mod nilawayModule) nilawayModuleResult {
		return runNilawayInModule(nilawayBin, mod)
	})

	var allIssues []string
	pkgCount := 0
	for _, res := range results {
		pkgCount += res.pkgCount
		if res.issue != "" {
			allIssues = append(allIssues, res.issue)
		}
	}

	if len(allIssues) > 0 {
		return CheckResult{}, fmt.Errorf("potential nil pointer issues found\n%s", indentOutput(strings.Join(allIssues, "\n")))
	}

	if pkgCount > 0 {
		result := Success(fmt.Sprintf("%d %s checked, no nil issues", pkgCount, Pluralize(pkgCount, "package", "packages")))
		result.Total = pkgCount
		return result, nil
	}
	return Success("No nil issues"), nil
}

// nilawayModules flattens FindAllGoModules' map in GetGoDirectories order (map iteration
// order is random), keeping each tree's modules in the order they were found.
func nilawayModules(rootDir string, allModules map[string][]string) []nilawayModule {
	var mods []nilawayModule
	for _, goDir := range GetGoDirectories() {
		for _, mod := range allModules[goDir] {
			mods = append(mods, nilawayModule{
				dir:   filepath.Join(rootDir, goDir, mod),
				label: filepath.Join(goDir, mod),
			})
		}
	}
	return mods
}

// runNilawayInModule counts the module's packages and runs nilaway over them.
func runNilawayInModule(nilawayBin string, mod nilawayModule) nilawayModuleResult {
	var res nilawayModuleResult

	listCmd := exec.Command("go", "list", "./...")
	listCmd.Dir = mod.dir
	listOutput, _ := RunCommand(listCmd, true)
	if trimmed := strings.TrimSpace(listOutput); trimmed != "" {
		res.pkgCount = len(strings.Split(trimmed, "\n"))
	}

	cmd := exec.Command(nilawayBin, "./...")
	cmd.Dir = mod.dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		issueText := strings.TrimSpace(output)
		if issueText == "" {
			issueText = err.Error()
		}
		res.issue = fmt.Sprintf("[%s]\n%s", mod.label, issueText)
	}
	return res
}

// runPerModule calls run for every item on at most `workers` goroutines and returns the
// results in input order, whatever order they finished in.
func runPerModule[T, R any](items []T, workers int, run func(T) R) []R {
	results := make([]R, len(items))
	if workers < 1 {
		workers = 1
	}
	jobs := make(chan int)
	var wg sync.WaitGroup
	for range min(workers, len(items)) {
		wg.Go(func() {
			for i := range jobs {
				results[i] = run(items[i])
			}
		})
	}
	for i := range items {
		jobs <- i
	}
	close(jobs)
	wg.Wait()
	return results
}
