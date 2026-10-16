#!/usr/bin/env sh
# Opens WEPANET in your browser (macOS / Linux). Needs python3, which these systems include.
# Browsers refuse to run WebAssembly from file:// pages, hence the small local server.
cd "$(dirname "$0")/web-app" || exit 1
PORT=8780
URL="http://localhost:$PORT/"
echo "WEPANET is running at $URL — press Ctrl+C to stop."
( sleep 1; (command -v open >/dev/null && open "$URL") || xdg-open "$URL" >/dev/null 2>&1 ) &
# Help's Manual and Tutorial are kept once, in docs/
exec python3 - "$PORT" <<'EOF'
import http.server, os, sys

DOCS = os.path.abspath("../docs")

class Handler(http.server.SimpleHTTPRequestHandler):
    def translate_path(self, path):
        local = super().translate_path(path)
        name = os.path.basename(local)
        if not os.path.exists(local) and name in ("Manual.html", "Tutorial.html"):
            return os.path.join(DOCS, name)
        return local

http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
EOF
