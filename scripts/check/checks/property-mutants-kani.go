package checks

import (
	"fmt"
	"os/exec"
	"strings"
)

// kaniPropertyKind: every `#[kani::proof]` is a property,
// `kani:<package>/<module path>::<fn>`. Its catcher is the proof itself: a proof
// covers every input up to its bound, so a record needs no `input`.
var kaniPropertyKind = propertyKind{
	name: "kani",
	properties: func(rootDir string) ([]string, error) {
		proofs, err := findKaniProofs(rootDir)
		if err != nil {
			return nil, err
		}
		names := make([]string, len(proofs))
		for i, proof := range proofs {
			names[i] = proof.property()
		}
		return names, nil
	},
	checkRecord: func(_ string, r mutantRecord) []string {
		if r.Input != "" {
			return []string{"`input` is set, but a proof takes no input; drop it"}
		}
		return nil
	},
	prepare: ensureKani,
	run:     runKaniMutantCatcher,
}

// runKaniMutantCatcher runs the record's one proof in tree. A failed check is
// the property going red; no verdict at all means the mutant didn't build.
func runKaniMutantCatcher(tree string, r mutantRecord) (mutantRun, error) {
	_, name := r.kindAndName()
	pkg, harness, ok := parseKaniProperty(name)
	if !ok {
		return mutantRun{}, fmt.Errorf("%q isn't `<package>/<harness path>`", name)
	}
	cmd := exec.Command("cargo", "kani", "-p", pkg, "--lib", "--harness", harness, "--exact", "--output-format", "terse")
	cmd.Dir = tree
	output, runErr := RunCommand(cmd, true)
	switch {
	case strings.Contains(output, "VERIFICATION:- FAILED"):
		return mutantRun{failed: true, output: kaniFindingReport(output)}, nil
	case runErr != nil:
		return mutantRun{}, fmt.Errorf("the proof doesn't build\n%s", indentOutput(trimBuildNoise(output)))
	default:
		return mutantRun{output: output}, nil
	}
}
