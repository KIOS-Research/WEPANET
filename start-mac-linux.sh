#!/usr/bin/env sh
# Opens WEPANET in your browser (macOS / Linux). Needs python3, which these systems include.
# Browsers refuse to run WebAssembly from file:// pages, hence the small local server.
cd "$(dirname "$0")/web-app" || exit 1
PORT=8780
URL="http://localhost:$PORT/"
echo "WEPANET is running at $URL — press Ctrl+C to stop."
( sleep 1; (command -v open >/dev/null && open "$URL") || xdg-open "$URL" >/dev/null 2>&1 ) &
exec python3 -m http.server "$PORT" --bind 127.0.0.1
