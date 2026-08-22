package checks

// The Docker fixture stacks a check can ask for, in the vocabulary a
// `CheckDefinition` uses. The stacks themselves — compose project, lease
// namespace, service tables — live in `scripts/check/stacklease`; this side
// carries only what a check declares.

// StackMode names one Docker fixture stack and the service set a check needs
// from it. Both strings are resolved against the `stacklease` registry, which
// owns the service tables; `TestEveryDeclaredStackModeResolves` is what keeps a
// typo here from becoming a runtime surprise.
type StackMode struct {
	Stack string
	Mode  string
}

func (s StackMode) String() string { return s.Stack + "/" + s.Mode }

// The fixture stack + mode pairs checks ask for. Each mirrors a mode in that
// stack's `stacklease` service table.
var (
	// SmbCore is the SMB integration set: guest, auth, both, readonly, flaky,
	// slow, maxreadsize, 50shares, unicode.
	SmbCore = StackMode{Stack: "smb", Mode: "core"}
	// SmbE2E is what the Linux Docker E2E suite talks to: guest, auth, 50shares,
	// unicode.
	SmbE2E = StackMode{Stack: "smb", Mode: "e2e"}
	// SftpCore is the SFTP integration set: every one of the eleven fixture
	// servers, because the lane runs the whole `cmdr-sftp` package and each
	// server exists for a cell in it.
	SftpCore = StackMode{Stack: "sftp", Mode: "core"}
)
