package checks

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// writeOpsSubtree is the engine this check fences. Relative to a member's `src/`.
const writeOpsSubtree = "file_system/write_operations"

// agentPathPattern matches a Rust path naming the `agent` module: `crate::agent::`,
// `super::agent::`, a bare `agent::`, or a `use` of any of them. The `\b` keeps
// identifiers that merely END in "agent" (a `user_agent::` helper) out of it.
var agentPathPattern = regexp.MustCompile(`\bagent::`)

type agentReachSite struct {
	relPath string
	line    int
	text    string
}

