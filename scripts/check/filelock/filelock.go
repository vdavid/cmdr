// Package filelock is the check runner's one exclusive file lock, with an arm per
// OS: the fixture-stack leases (`stacklease`) and the CSV logs (`stats.go`) hold it
// across a critical section. The lock belongs to the open file, so closing the
// file drops it even if the process later forks.
package filelock
