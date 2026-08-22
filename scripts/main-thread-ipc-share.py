#!/usr/bin/env python3
"""How much of the main thread a `sample` window found inside IPC command handling.

`ps -M` can't answer this: its thread rows aren't ordered, so there is no way to
point at the main thread, and process-wide CPU is swamped by index-writer churn
on other threads (`docs/notes/idle-cpu-attribution-2026-08-03.md`).

`sample` can. It names the main thread, and on macOS a Tauri IPC command arrives
through wry's `url_scheme_handler::start_task`, so the share of main-thread
samples under that frame IS the share of the main thread that is answering IPC
rather than free to draw, scroll, and take keystrokes.

Usage: main-thread-ipc-share.py <sample-output-file>
"""

import re
import sys

MAIN_THREAD = "com.apple.main-thread"
IPC_ENTRY = "url_scheme_handler10start_task"


def main_thread_block(text: str) -> tuple[int, list[str]]:
    """The main thread's call tree, plus the total samples it holds."""
    start = text.index(MAIN_THREAD)
    header_line_start = text.rindex("\n", 0, start) + 1
    total = int(text[header_line_start:start].split()[0])
    block = text[start:]
    following_thread = re.search(r"\n\s*\d+ Thread_", block)
    if following_thread:
        block = block[: following_thread.start()]
    return total, block.splitlines()


def ipc_samples(lines: list[str]) -> int:
    """Samples under the OUTERMOST `start_task` frames, never double-counted.

    The frame recurses (one nested call per IPC message in flight), so summing
    every occurrence would count the same samples several times over. Indent
    depth is what tells an outer frame from an inner one.
    """
    total = 0
    open_at: int | None = None
    for line in lines:
        match = re.match(r"^(\s*[+!:|\s]*?)(\d+) (\S+)", line)
        if not match:
            continue
        depth, count, symbol = len(match.group(1)), int(match.group(2)), match.group(3)
        if open_at is not None and depth <= open_at:
            open_at = None
        if open_at is None and IPC_ENTRY in symbol:
            total += count
            open_at = depth
    return total


if __name__ == "__main__":
    text = open(sys.argv[1]).read()
    total, lines = main_thread_block(text)
    ipc = ipc_samples(lines)
    print(f"main thread: {ipc}/{total} samples answering IPC = {ipc / total * 100:.1f}%")
