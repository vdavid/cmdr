package main

import (
	"bufio"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// The decisions behind retrying a failed gh call. Many parallel lands poll one account, so its
// API budget does run out, and failing then would throw away a land whose CI already went green.

// apiBudget is what GitHub says about the budget a failed call ran into.
type apiBudget struct {
	spent bool
	// reset is when a spent budget refills.
	reset time.Time
	// retryAfter is a secondary rate limit's `Retry-After`.
	retryAfter time.Duration
	// limited: GitHub turned the call away over a rate limit (403 or 429) without saying when to
	// come back, which is a secondary limit with budget left. Its docs say to wait at least a
	// minute, then back off further each time.
	limited bool
}

type ghRetry int

const (
	ghGiveUp ghRetry = iota
	// ghWaitForReset: the API budget is spent; wait until it refills, then retry.
	ghWaitForReset
	// ghRetryTransient: budget left, so likely a network blip; retry shortly, a few times.
	ghRetryTransient
	// ghBackOff: a secondary limit with no `Retry-After`; wait a minute, doubling each time.
	ghBackOff
)

const (
	resetSlack       = 5 * time.Second
	transientRetries = 2
	transientWait    = 10 * time.Second
	// A secondary limit's backoffs: 1, 2, 4, and 8 min, 15 min in all, then give up.
	secondaryBackoff  = time.Minute
	secondaryBackoffs = 4
)

// afterGHFailure decides what follows a failed gh call, given GitHub's word on the budget (nil
// when unknown), how many short retries the call has left, and how many times it already backed
// off a secondary limit. A rate limit always waits, even for a write with no retries: the call
// never reached GitHub's handler.
func afterGHFailure(b *apiBudget, retriesLeft, backedOff int, now time.Time) (ghRetry, time.Duration) {
	switch {
	case b != nil && b.retryAfter > 0:
		return ghWaitForReset, b.retryAfter + resetSlack
	case b != nil && b.spent:
		return ghWaitForReset, max(b.reset.Sub(now), 0) + resetSlack
	case b != nil && b.limited && backedOff < secondaryBackoffs:
		return ghBackOff, secondaryBackoff << backedOff
	case b != nil && b.limited:
		return ghGiveUp, 0
	case retriesLeft > 0:
		return ghRetryTransient, transientWait
	default:
		return ghGiveUp, 0
	}
}

// apiResponse is what `gh api --include` printed: the status, the budget headers, the ETag a
// later poll can send back, and the body.
type apiResponse struct {
	status int
	budget apiBudget
	etag   string
	body   string
}

// parseAPIResponse splits `gh api --include` output into its parts. ok is false when there's no
// HTTP response in it (gh never reached GitHub).
func parseAPIResponse(out string) (apiResponse, bool) {
	resp, err := http.ReadResponse(bufio.NewReader(strings.NewReader(out)), nil)
	if err != nil {
		return apiResponse{}, false
	}
	// gh prints no Content-Length, so the body runs to the end.
	raw, err := io.ReadAll(resp.Body)
	_ = resp.Body.Close()
	if err != nil {
		return apiResponse{}, false
	}
	r := apiResponse{status: resp.StatusCode, etag: resp.Header.Get("Etag"), body: string(raw)}
	h := resp.Header
	if remaining, err := strconv.Atoi(h.Get("X-RateLimit-Remaining")); err == nil && remaining == 0 {
		r.budget.spent = true
		if reset, err := strconv.ParseInt(h.Get("X-RateLimit-Reset"), 10, 64); err == nil {
			r.budget.reset = time.Unix(reset, 0)
		}
	}
	if secs, err := strconv.Atoi(h.Get("Retry-After")); err == nil && secs > 0 {
		r.budget.retryAfter = time.Duration(secs) * time.Second
	}
	limitedStatus := r.status == http.StatusForbidden || r.status == http.StatusTooManyRequests
	r.budget.limited = limitedStatus && !r.budget.spent && r.budget.retryAfter == 0
	return r, true
}

// responseCache holds each polled path's last 200 body and its ETag. Sending the ETag back as
// `If-None-Match` turns an unchanged answer into a 304, which GitHub doesn't charge to the
// primary budget (verified 2026-10-21: `X-RateLimit-Remaining` stayed at 4,942 across two 304s
// on the workflow-runs list). Most polls of a running CI see no change, so most cost nothing.
type responseCache map[string]cachedResponse

type cachedResponse struct {
	etag string
	body string
}

// etag is the ETag to send for path, or "" with nothing cached. A nil cache caches nothing.
func (c responseCache) etag(path string) string {
	return c[path].etag
}

// settle reads a response to path: a 2xx is the body (and refreshes the cache), a 304 is the
// cached body. ok is false for anything else, or a 304 with nothing cached.
func (c responseCache) settle(path string, r apiResponse) (body string, ok bool) {
	switch {
	case r.status >= 200 && r.status < 300:
		if c != nil && r.etag != "" {
			c[path] = cachedResponse{etag: r.etag, body: r.body}
		}
		return r.body, true
	case r.status == http.StatusNotModified:
		cached, found := c[path]
		return cached.body, found
	default:
		return "", false
	}
}

// rateLimit is `gh api rate_limit`, a call that costs no budget itself. `gh pr` spends the
// GraphQL budget and `gh run` the REST (core) one. It can disagree with the headers of the call
// that failed (2026-10-20: `core` showed 2,862 left while the Actions API answered remaining 0),
// so it's only the fallback for calls with no headers of their own.
type rateLimit struct {
	Resources struct {
		Core    rateBucket `json:"core"`
		GraphQL rateBucket `json:"graphql"`
	} `json:"resources"`
}

type rateBucket struct {
	Remaining int   `json:"remaining"`
	Reset     int64 `json:"reset"` // Unix seconds
}

// budget is the latest-refilling spent bucket, or an unspent budget.
func (l rateLimit) budget() apiBudget {
	var b apiBudget
	for _, bucket := range []rateBucket{l.Resources.Core, l.Resources.GraphQL} {
		if reset := time.Unix(bucket.Reset, 0); bucket.Remaining == 0 && (!b.spent || reset.After(b.reset)) {
			b = apiBudget{spent: true, reset: reset}
		}
	}
	return b
}
