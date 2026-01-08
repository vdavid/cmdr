#!/bin/sh
set -e

UP_SECONDS=${UP_SECONDS:-5}
DOWN_SECONDS=${DOWN_SECONDS:-5}

echo "Flaky server starting (${UP_SECONDS}s up, ${DOWN_SECONDS}s down)..."

while true; do
    echo "$(date): Samba UP for ${UP_SECONDS}s"
    smbd --foreground --no-process-group --debug-stdout &
    SMB_PID=$!
    sleep "$UP_SECONDS"
    
    echo "$(date): Samba DOWN for ${DOWN_SECONDS}s"
    kill $SMB_PID 2>/dev/null || true
    wait $SMB_PID 2>/dev/null || true
    sleep "$DOWN_SECONDS"
done
