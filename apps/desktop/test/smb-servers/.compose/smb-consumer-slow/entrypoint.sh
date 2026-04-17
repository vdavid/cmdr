#!/bin/sh
# Add 200ms latency to loopback (affects all traffic in/out of container).
tc qdisc add dev eth0 root netem delay 200ms 2>/dev/null || true
exec smbd --foreground --no-process-group --debug-stdout
