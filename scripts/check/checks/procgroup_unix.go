//go:build unix

package checks

import "syscall"

// ownProcessGroup starts a child as the leader of a new process group, so
// signalling the group reaches every descendant it spawns (cargo's rustc, pnpm's
// node).
func ownProcessGroup() *syscall.SysProcAttr {
	return &syscall.SysProcAttr{Setpgid: true}
}

// stopProcessGroup asks the group led by pid to exit (SIGTERM).
func stopProcessGroup(pid int) {
	_ = syscall.Kill(-pid, syscall.SIGTERM)
}

// killProcessGroup ends the group led by pid at once (SIGKILL).
func killProcessGroup(pid int) {
	_ = syscall.Kill(-pid, syscall.SIGKILL)
}
