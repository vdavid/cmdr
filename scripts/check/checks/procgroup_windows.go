package checks

import (
	"os/exec"
	"strconv"
	"syscall"
)

// ownProcessGroup starts a child in a new process group, the Windows twin of
// Setpgid: a console Ctrl+C then reaches only the runner, which ends each
// tracked tree itself, as on Unix.
func ownProcessGroup() *syscall.SysProcAttr {
	return &syscall.SysProcAttr{CreationFlags: syscall.CREATE_NEW_PROCESS_GROUP}
}

// stopProcessGroup ends the process tree rooted at pid. Windows has no SIGTERM
// to send a console process tree, so this is `killProcessGroup`.
func stopProcessGroup(pid int) {
	killProcessGroup(pid)
}

// killProcessGroup ends the process tree rooted at pid (`taskkill /T /F`).
func killProcessGroup(pid int) {
	_ = exec.Command("taskkill", "/T", "/F", "/PID", strconv.Itoa(pid)).Run()
}
