#!/usr/bin/env python3
"""How much of the main thread a `sample` window found inside IPC command handling.

`ps -M` can't answer this: its thread rows aren't ordered, so there is no way to
point at the main thread, and process-wide CPU is swamped by index-writer churn
on other threads (`docs/notes/idle-cpu-attribution-2026-08-03.md`).

`sample` can. It names the main thread, and a Tauri IPC command enters it through
one of wry's two transports, so the share of main-thread samples under that entry
frame IS the share of the main thread that is answering IPC rather than free to
draw, scroll, and take keystrokes.

⚠️ **Which transport carries the command differs between builds**, so matching only
one of them silently reports 0.0% on the other. A debug build routes the command
through the custom URL scheme (`wry::…url_scheme_handler::start_task`); a release
build routes it through the WebKit script-message handler
(`wry::…did_receive` → `tauri_runtime_wry::create_ipc_handler`). Both markers are
listed below, and a file with neither is a hard error rather than a 0%.

Usage: main-thread-ipc-share.py <sample-output-file>
"""

import re
import sys

MAIN_THREAD = "com.apple.main-thread"

#: Frames a Tauri IPC command can enter the main thread through. See the module doc:
#: debug and release builds do NOT use the same one.
IPC_ENTRIES = ("url_scheme_handler10start_task", "create_ipc_handler")


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


def ipc_samples(lines: list[str]) -> tuple[int, str | None]:
    """Samples under the OUTERMOST IPC entry frames, never double-counted.

    The frame recurses (one nested call per IPC message in flight), so summing
    every occurrence would count the same samples several times over. Indent
    depth is what tells an outer frame from an inner one.

    Also returns which marker matched, so the caller can say "no IPC frame at
    all" apart from "the IPC frame held no samples".
    """
    total = 0
    matched: str | None = None
    open_at: int | None = None
    for line in lines:
        match = re.match(r"^(\s*[+!:|\s]*?)(\d+) (\S+)", line)
        if not match:
            continue
        depth, count, symbol = len(match.group(1)), int(match.group(2)), match.group(3)
        if open_at is not None and depth <= open_at:
            open_at = None
        entry = next((e for e in IPC_ENTRIES if e in symbol), None) if open_at is None else None
        if entry:
            total += count
            matched = entry
            open_at = depth
    return total, matched


if __name__ == "__main__":
    text = open(sys.argv[1]).read()
    total, lines = main_thread_block(text)
    ipc, entry = ipc_samples(lines)
    if entry is None:
        # A 0% here would read as "the main thread was free", which is the opposite
        # of what an unrecognized transport means. Fail instead.
        raise SystemExit(
            f"no IPC entry frame found on the main thread (looked for {', '.join(IPC_ENTRIES)}). "
            "If wry changed transports, add the new marker to IPC_ENTRIES."
        )
    print(f"main thread: {ipc}/{total} samples answering IPC = {ipc / total * 100:.1f}% (via {entry})")
