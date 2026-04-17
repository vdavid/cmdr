#!/bin/sh
# Cycle smbd: 5s up, 5s down. Tests reconnect behavior.
while true; do
    smbd --foreground --no-process-group --debug-stdout &
    PID=$!
    sleep 5
    kill "$PID" 2>/dev/null
    wait "$PID" 2>/dev/null
    sleep 5
done
