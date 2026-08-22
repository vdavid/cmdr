#!/usr/bin/env python3
"""A byte-level proxy in front of the REAL `sftp-server`, so a fixture can be a
server with a quirk rather than a different SFTP implementation.

OpenSSH's `sftp-server` has no switches for any of this: it always advertises
`posix-rename@openssh.com`, always answers `limits@openssh.com` with its own
numbers, and never short-reads. Swapping in a third-party server to get those
behaviours would mean testing against something users don't run. Proxying keeps
every other byte identical to stock OpenSSH.

Quirks come from `/etc/sftp-quirks` (sshd scrubs the subsystem's environment):

  QUIRK_DROP_EXTENSIONS=posix-rename@openssh.com,copy-data
      Removed from the SSH_FXP_VERSION hello, so a client's `support_*`
      predicate answers false and its fallback path runs. Names must match what
      the server actually sends: `copy-data` has no `@openssh.com` suffix, and a
      name that matches nothing drops nothing, silently.

  QUIRK_SHORT_READ_BYTES=4096
      Every SSH_FXP_DATA is truncated to at most this many bytes. Legal SFTP: a
      server may return fewer bytes than asked for. Catches a reader that
      advances its offset by the REQUESTED length instead of the returned one,
      which silently holes a file and duplicates bytes.

  QUIRK_LIMITS=max-packet:16384,max-read:8192,max-write:8192,max-handles:16
      Rewrites the `limits@openssh.com` reply, for a client that has to respect
      a server far stingier than OpenSSH's defaults.

SFTP framing is a 4-byte big-endian length followed by that many bytes, in both
directions, which is all the parsing this needs.
"""

import os
import struct
import sys

SSH_FXP_VERSION = 2
SSH_FXP_DATA = 103
SSH_FXP_EXTENDED = 200
SSH_FXP_EXTENDED_REPLY = 201

LIMITS = b"limits@openssh.com"


def load_quirks(path="/etc/sftp-quirks"):
    quirks = {}
    try:
        with open(path) as handle:
            for line in handle:
                line = line.strip()
                if "=" in line:
                    name, value = line.split("=", 1)
                    quirks[name] = value
    except FileNotFoundError:
        pass
    return quirks


def read_exactly(stream, count):
    chunks = []
    while count:
        chunk = stream.read(count)
        if not chunk:
            return None
        chunks.append(chunk)
        count -= len(chunk)
    return b"".join(chunks)


def read_packet(stream):
    header = read_exactly(stream, 4)
    if header is None:
        return None
    return read_exactly(stream, struct.unpack(">I", header)[0])


def write_packet(stream, payload):
    stream.write(struct.pack(">I", len(payload)) + payload)
    stream.flush()


def read_string(payload, at):
    (length,) = struct.unpack_from(">I", payload, at)
    at += 4
    return payload[at : at + length], at + length


def drop_extensions(payload, unwanted):
    """Rewrite SSH_FXP_VERSION, keeping every extension pair but the named ones."""
    version = payload[1:5]
    at = 5
    kept = []
    while at < len(payload):
        name, at = read_string(payload, at)
        data, at = read_string(payload, at)
        if name.decode("utf-8", "replace") not in unwanted:
            kept.append(struct.pack(">I", len(name)) + name + struct.pack(">I", len(data)) + data)
    return bytes([SSH_FXP_VERSION]) + version + b"".join(kept)


def rewrite_limits(payload, limits):
    """Rewrite the four uint64s of a `limits@openssh.com` reply."""
    request_id = payload[1:5]
    packed = payload[5:]
    if len(packed) != 32:
        return payload
    current = list(struct.unpack(">QQQQ", packed))
    for index, name in enumerate(("max-packet", "max-read", "max-write", "max-handles")):
        if name in limits:
            current[index] = limits[name]
    return bytes([SSH_FXP_EXTENDED_REPLY]) + request_id + struct.pack(">QQQQ", *current)


def main():
    quirks = load_quirks()
    unwanted = {name for name in quirks.get("QUIRK_DROP_EXTENSIONS", "").split(",") if name}
    short_read = int(quirks.get("QUIRK_SHORT_READ_BYTES", "0"))
    limits = {}
    for pair in quirks.get("QUIRK_LIMITS", "").split(","):
        if ":" in pair:
            name, value = pair.split(":", 1)
            limits[name] = int(value)

    import subprocess
    import threading

    server = subprocess.Popen(
        ["/usr/lib/ssh/sftp-server"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
    )

    # Request ids whose reply is a `limits` answer, so only that reply is
    # rewritten and every other EXTENDED_REPLY passes through untouched.
    limits_requests = set()

    def client_to_server():
        while True:
            payload = read_packet(sys.stdin.buffer)
            if payload is None:
                break
            if limits and payload and payload[0] == SSH_FXP_EXTENDED:
                name, _ = read_string(payload, 5)
                if name == LIMITS:
                    limits_requests.add(payload[1:5])
            write_packet(server.stdin, payload)
        try:
            server.stdin.close()
        except BrokenPipeError:
            pass

    def server_to_client():
        while True:
            payload = read_packet(server.stdout)
            if payload is None:
                break
            kind = payload[0] if payload else None
            if kind == SSH_FXP_VERSION and unwanted:
                payload = drop_extensions(payload, unwanted)
            elif kind == SSH_FXP_DATA and short_read:
                request_id = payload[1:5]
                (length,) = struct.unpack_from(">I", payload, 5)
                if length > short_read:
                    data = payload[9 : 9 + short_read]
                    payload = bytes([SSH_FXP_DATA]) + request_id + struct.pack(">I", len(data)) + data
            elif kind == SSH_FXP_EXTENDED_REPLY and payload[1:5] in limits_requests:
                limits_requests.discard(payload[1:5])
                payload = rewrite_limits(payload, limits)
            write_packet(sys.stdout.buffer, payload)

    upstream = threading.Thread(target=client_to_server, daemon=True)
    upstream.start()
    server_to_client()
    server.wait()


if __name__ == "__main__":
    main()
