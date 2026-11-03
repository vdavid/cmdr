package checks

import (
	"errors"
	"fmt"
	"os/exec"
	"sort"
)

// shuttlePropertyKind: every interleaving test (`fn shuttle_*`, see
// `desktop-rust-shuttle.go`) is a property, named by its function. Its catcher
// is that one test, built and run the way the lane runs it. The tests explore a
// few hundred schedules each, so a mutant that needs a particular interleaving
// is still caught on every run, not on a lucky one.
var shuttlePropertyKind = propertyKind{
	name:       "shuttle",
	properties: shuttlePropertyNames,
	// A Shuttle record takes no field of its own: the test IS the input.
	checkRecord: func(_ string, r mutantRecord) []string {
		if r.Input != "" {
			return []string{"`input` is set; a shuttle record runs its test, which needs none"}
		}
		return nil
	},
	prepare: EnsureCargoNextest,
	run:     runShuttleMutantCatcher,
}

// shuttlePropertyNames lists every interleaving test, sorted.
func shuttlePropertyNames(rootDir string) ([]string, error) {
	tests, err := shuttleTests(rootDir)
	if err != nil {
		return nil, err
	}
	names := make([]string, 0, len(tests))
	for name := range tests {
		names = append(names, name)
	}
	sort.Strings(names)
	return names, nil
}

// runShuttleMutantCatcher runs the record's one interleaving test in tree. A
// red nextest after a good build is the property failing; a build failure is
// told apart by asking cargo to build first.
func runShuttleMutantCatcher(tree string, r mutantRecord) (mutantRun, error) {
	_, name := r.kindAndName()
	tests, err := shuttleTests(tree)
	if err != nil {
		return mutantRun{}, err
	}
	c, ok := tests[name]
	if !ok {
		return mutantRun{}, fmt.Errorf("no interleaving test named %s in the tree", name)
	}
	filter := fmt.Sprintf("test(/::%s$/)", name)
	if output, err := RunCommand(shuttleCargo(tree, c.pkg, []string{"nextest", "run"}, "--no-run"), true); err != nil {
		return mutantRun{}, fmt.Errorf("the test doesn't build\n%s", indentOutput(trimBuildNoise(output)))
	}
	output, err := RunCommand(shuttleNextest(tree, c.pkg, filter), true)
	var exitErr *exec.ExitError
	switch {
	case err == nil:
		return mutantRun{output: output}, nil
	case errors.As(err, &exitErr):
		return mutantRun{failed: true, output: output}, nil
	default:
		return mutantRun{}, fmt.Errorf("couldn't run the test: %w", err)
	}
}
