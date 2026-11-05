package main

import (
	"errors"
	"fmt"
	"time"
)

// Inside Claude Code, `land` stops waiting on purpose before the agent that runs it idles past
// its prompt cache's lifetime (an hour): the agent sleeps until `land` exits, and a cache that
// expired is costly to rebuild. Running `land` again picks the same run back up (`DETAILS.md`
// § Decision: yield before the prompt cache expires).

const (
	// claudeYieldAfter: how long after it started `land` waits before yielding inside Claude
	// Code, with margin under the cache's hour.
	claudeYieldAfter = 50 * time.Minute
	// exitStillWaiting is `land`'s exit status when it yielded: CI is still running, nothing failed.
	exitStillWaiting = 75
	// claudeCodeEnv is set in every shell Claude Code runs.
	claudeCodeEnv = "CLAUDECODE"
)

// processStart is when this `land` started: the yield counts from here, across every CI run it
// waits through.
var processStart = time.Now()

// stillWaiting: `land` yielded while CI still runs (`shouldYield`). Its message says when to
// expect `CI OK` and how to keep waiting.
type stillWaiting struct{ msg string }

func (e stillWaiting) Error() string { return e.msg }

// isStillWaiting reports whether err is a yield, which exits with `exitStillWaiting`.
func isStillWaiting(err error) bool {
	var sw stillWaiting
	return errors.As(err, &sw)
}

// defaultYieldAfter is `-yield-after`'s default, from `CLAUDECODE`'s value: 0 (never) outside
// Claude Code.
func defaultYieldAfter(claudeCode string) time.Duration {
	if claudeCode == "" {
		return 0
	}
	return claudeYieldAfter
}

// shouldYield says whether a wait gives up for now, given how long the process has run.
func shouldYield(yieldAfter, running time.Duration) bool {
	return yieldAfter > 0 && running >= yieldAfter
}

// yieldMessage is what `land` prints when it yields: the ETA it last printed, the queue if
// any, and how to keep waiting.
func yieldMessage(est ciEstimate, now time.Time, again string) string {
	eta := "no ETA yet"
	if !est.typical.IsZero() {
		eta = fmt.Sprintf("expect it around %s, in about %s, up to %s", est.typical.Local().Format("15:04"), mins(est.typical.Sub(now)), mins(est.high.Sub(now)))
		if est.queueWait > 0 {
			eta += fmt.Sprintf("; %d %s ahead of ours in the %s queue", est.queuedAhead, plural(est.queuedAhead, "job", "jobs"), runnerNames[est.queuedClass])
		}
	}
	return fmt.Sprintf("Still waiting for %s (%s). Run `%s` again to keep waiting.", ciCheck, eta, again)
}
