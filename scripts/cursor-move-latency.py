#!/usr/bin/env python3
"""How long a cursor move takes at a given depth in a big listing, end to end.

A cursor move is what drives the MCP pane mirror, and the mirror fetches the
pane's visible range. That is the user-visible shape of the listing wedge
(`docs/notes/listing-row-fetch-quadratic-2026-08-22.md`): the fetch used to cost
one full walk of the listing per row, so a pane parked deep in a big directory
answered a keystroke in seconds.

Reports how many moves the app ANSWERED inside the tool's acknowledgement
deadline, and the median of those. A median over every timing would be a median
of ceilings: an unanswered move reads as exactly the deadline, so counting them
is the only honest way to say the app stopped responding.

Runs both depths in one process, in alternating blocks, so a background index
storm hits both arms rather than whichever one it happened to overlap
(`docs/notes/idle-cpu-attribution-2026-08-03.md`: never order work off one
window here).

Usage: cursor-move-latency.py <shallow-row> <deep-row> [rounds]
Environment: CMDR_INSTANCE_ID (default `dev`), as for `mcp-call.sh`.
"""

import http.client
import json
import os
import statistics
import sys
import time

INSTANCE = os.environ.get("CMDR_INSTANCE_ID", "dev")
DATA_DIR = os.path.expanduser(f"~/Library/Application Support/com.veszelovszki.cmdr-{INSTANCE}")

#: What `move_cursor` waits for a frontend acknowledgement before answering anyway.
ACK_DEADLINE_MS = 5_000


def read(name: str) -> str:
    with open(os.path.join(DATA_DIR, name)) as handle:
        return handle.read().strip()


class Mcp:
    """One keep-alive connection, so the timings are the app's and not curl's."""

    def __init__(self) -> None:
        self.token = read("mcp.token")
        self.conn = http.client.HTTPConnection("127.0.0.1", int(read("mcp.port")), timeout=150)
        self.next_id = 0
        # The server wants an `initialize` before it will take a `tools/call`.
        self.rpc(
            "initialize",
            {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "cursor-move-latency", "version": "1.0"},
            },
        )

    def rpc(self, method: str, params: dict) -> dict:
        self.next_id += 1
        body = json.dumps({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        self.conn.request(
            "POST",
            "/mcp",
            body,
            {"Content-Type": "application/json", "Authorization": f"Bearer {self.token}"},
        )
        response = self.conn.getresponse()
        raw = response.read().decode()
        if response.status // 100 != 2:
            raise SystemExit(f"MCP answered HTTP {response.status}: {raw[:400]}")
        return json.loads(raw)

    def call(self, tool: str, arguments: dict) -> dict:
        return self.rpc("tools/call", {"name": tool, "arguments": arguments})


def move(mcp: Mcp, row: int) -> float:
    started = time.perf_counter()
    mcp.call("move_cursor", {"pane": "left", "index": row})
    return (time.perf_counter() - started) * 1000


def block(mcp: Mcp, first: int, moves: int) -> list[float]:
    """`moves` one-row steps starting at `first`, each timed.

    A BLOCK per depth, never alternating row-by-row: a move that jumps the cursor
    across the whole listing re-renders and re-fetches, which would put that cost
    in both arms and hide the thing being measured.
    """
    mcp.call("scroll_to", {"pane": "left", "index": first})
    move(mcp, first)
    return [move(mcp, first + step) for step in range(moves)]


def main() -> None:
    shallow, deep = int(sys.argv[1]), int(sys.argv[2])
    rounds = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    moves = 5
    mcp = Mcp()

    timings: dict[str, list[float]] = {"shallow": [], "deep": []}
    for round_index in range(rounds):
        timings["shallow"] += block(mcp, shallow + round_index * moves, moves)
        timings["deep"] += block(mcp, deep + round_index * moves, moves)

    for label, row in (("shallow", shallow), ("deep", deep)):
        samples = timings[label]
        # `move_cursor` gives up waiting for the frontend to acknowledge at 5 s and
        # answers anyway, so a timing at the ceiling means the app never answered.
        # Count those separately: a median made of ceilings is not a latency.
        answered = [s for s in samples if s < ACK_DEADLINE_MS]
        print(
            f"row {row:>6}: answered {len(answered)}/{len(samples)} moves within {ACK_DEADLINE_MS / 1000:.0f}s"
            + (
                f", median {statistics.median(answered):5.0f} ms, min {min(answered):5.0f} ms"
                if answered
                else " (the app never answered)"
            )
        )


if __name__ == "__main__":
    main()
