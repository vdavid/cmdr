package checks

import (
	"encoding/json"
	"strings"
)

// clippyDiagnostic is the slice of rustc's JSON diagnostic the clippy lane reads: the text a
// human sees, and the suggestions `cargo clippy --fix` could apply.
type clippyDiagnostic struct {
	Rendered *string            `json:"rendered"`
	Spans    []clippySpan       `json:"spans"`
	Children []clippyDiagnostic `json:"children"`
}

type clippySpan struct {
	SuggestionApplicability *string `json:"suggestion_applicability"`
}

type clippyCargoMessage struct {
	Reason  string            `json:"reason"`
	Message *clippyDiagnostic `json:"message"`
}

// machineApplicable reports whether the diagnostic or any of its children carries a
// suggestion `cargo fix` applies: it applies `MachineApplicable` ones only.
func (d *clippyDiagnostic) machineApplicable() bool {
	for _, span := range d.Spans {
		if span.SuggestionApplicability != nil && *span.SuggestionApplicability == "MachineApplicable" {
			return true
		}
	}
	for i := range d.Children {
		if d.Children[i].machineApplicable() {
			return true
		}
	}
	return false
}

// parseClippyJSON turns a `cargo clippy --message-format=json` run's combined output into
// what a human reads (each diagnostic's rendered text, plus cargo's own stderr lines as they
// came) and whether any diagnostic is machine-fixable. The lane decides on that typed field,
// so a compile error or a lint with no applicable fix reports at once instead of paying for a
// `--fix` pass that can't help.
func parseClippyJSON(output string) (display string, fixable bool) {
	var b strings.Builder
	for _, line := range strings.Split(output, "\n") {
		if line == "" {
			continue
		}
		var msg clippyCargoMessage
		if !strings.HasPrefix(line, "{") || json.Unmarshal([]byte(line), &msg) != nil {
			b.WriteString(line)
			b.WriteByte('\n')
			continue
		}
		if msg.Reason != "compiler-message" || msg.Message == nil {
			continue
		}
		if msg.Message.machineApplicable() {
			fixable = true
		}
		if msg.Message.Rendered != nil {
			b.WriteString(strings.TrimRight(*msg.Message.Rendered, "\n"))
			b.WriteByte('\n')
		}
	}
	return b.String(), fixable
}
