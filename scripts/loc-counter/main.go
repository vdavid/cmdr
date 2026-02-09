package main

import (
	"encoding/csv"
	"fmt"
	"os"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

type job struct {
	date   string
	commit commit
}

type result struct {
	date  string
	stats *fileStats
	err   error
}

func main() {
	commits, err := getCommits()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error getting commits: %v\n", err)
		os.Exit(1)
	}

	dailyCommits := groupCommitsByDate(commits)
	if len(dailyCommits) == 0 {
		return
	}

	allDates := make([]string, 0, len(dailyCommits))
	for date := range dailyCommits {
		allDates = append(allDates, date)
	}
	sort.Strings(allDates)

	allConsecutiveDates := generateConsecutiveDates(allDates[0], allDates[len(allDates)-1])

	// Fan out: count lines for each commit day in parallel
	totalCommits := len(dailyCommits)
	results := processCommits(dailyCommits, totalCommits)

	// Write CSV in chronological order, filling gaps with previous day's stats
	writer := csv.NewWriter(os.Stdout)
	defer writer.Flush()

	header := []string{
		"date", "total",
		"rust", "ts",
		"rust prod", "rust test",
		"ts prod", "ts test",
		"svelte", "astro", "go",
		"css", "docs", "other",
		"comments",
	}
	if err := writer.Write(header); err != nil {
		fmt.Fprintf(os.Stderr, "Error writing CSV header: %v\n", err)
		return
	}

	var prevStats *fileStats
	for _, date := range allConsecutiveDates {
		var stats *fileStats
		var comments string

		if r, ok := results[date]; ok {
			if r.err != nil {
				fmt.Fprintf(os.Stderr, "Error on %s: %v\n", date, r.err)
				if prevStats != nil {
					stats = prevStats.copyWithoutComments()
					comments = "-"
				} else {
					continue
				}
			} else {
				stats = r.stats
				comments = strings.Join(stats.comments, "; ")
				prevStats = stats
			}
		} else {
			if prevStats == nil {
				continue
			}
			stats = prevStats.copyWithoutComments()
			comments = "-"
		}

		row := []string{
			date,
			strconv.Itoa(stats.total),
			strconv.Itoa(stats.rust),
			strconv.Itoa(stats.ts),
			strconv.Itoa(stats.rustProd),
			strconv.Itoa(stats.rustTest),
			strconv.Itoa(stats.tsProd),
			strconv.Itoa(stats.tsTest),
			strconv.Itoa(stats.svelte),
			strconv.Itoa(stats.astro),
			strconv.Itoa(stats.goTotal),
			strconv.Itoa(stats.css),
			strconv.Itoa(stats.docs),
			strconv.Itoa(stats.other),
			comments,
		}
		if err := writer.Write(row); err != nil {
			fmt.Fprintf(os.Stderr, "Error writing CSV row: %v\n", err)
			return
		}
	}
}

// processCommits fans out commit processing to a worker pool sized to CPU count.
func processCommits(dailyCommits map[string]commit, totalCommits int) map[string]result {
	workers := runtime.NumCPU()
	jobs := make(chan job, totalCommits)
	resultsCh := make(chan result, totalCommits)

	var done atomic.Int32
	fmt.Fprintf(os.Stderr, "Processing %d commits with %d workers...\n", totalCommits, workers)

	// Start workers
	var wg sync.WaitGroup
	for range workers {
		wg.Go(func() {
			for j := range jobs {
				stats, err := countLinesForCommit(j.commit.hash, j.commit.messages)
				resultsCh <- result{date: j.date, stats: stats, err: err}
				n := done.Add(1)
				fmt.Fprintf(os.Stderr, "\r[%d/%d] Processed %s", n, totalCommits, j.date)
			}
		})
	}

	// Send jobs
	for date, c := range dailyCommits {
		jobs <- job{date: date, commit: c}
	}
	close(jobs)

	// Wait for all workers, then close results channel
	wg.Wait()
	close(resultsCh)
	fmt.Fprintln(os.Stderr)

	// Collect results by date
	results := make(map[string]result, totalCommits)
	for r := range resultsCh {
		results[r.date] = r
	}
	return results
}

func generateConsecutiveDates(firstDate, lastDate string) []string {
	start, err := time.Parse("2006-01-02", firstDate)
	if err != nil {
		return []string{firstDate}
	}
	end, err := time.Parse("2006-01-02", lastDate)
	if err != nil {
		return []string{lastDate}
	}

	var dates []string
	current := start
	for !current.After(end) {
		dates = append(dates, current.Format("2006-01-02"))
		current = current.AddDate(0, 0, 1)
	}
	return dates
}
