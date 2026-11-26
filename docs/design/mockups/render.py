"""Render design-canvas artboards (.dc.html) to PNG with headless Chrome (#628). Standard library only.

  python -I docs/design/mockups/render.py m4                    # every artboard in m4/canvas.json -> m4/png/
  python -I docs/design/mockups/render.py m4 Main Pause43       # only these
  python -I docs/design/mockups/render.py m4 --out <dir>        # somewhere else
  python -I docs/design/mockups/render.py m5 --sheet sheet.png --scale 0.25 A1 A2 A3
                                                                # several artboards on one contact sheet
  python -I docs/design/mockups/render.py --serve               # browse http://127.0.0.1:8765/m4/Main.dc.html

Each folder's support.js loads dc-render.js, our own renderer for the format (not the canvas
runtime). canvas.json gives each artboard's size. Props go in the query string
(`Main.dc.html?state=offline`); on the command line `Main?state=offline` renders that state to
`Main-state-offline.png`.

Art: the canvas keeps uploads (the delicious fruit) as `/_blob/<id>` links. Those files are
fangame art and are not committed; put local copies in `docs/design/mockups/blobs/<id>.png`
(gitignored) to see them, otherwise a plain placeholder shape is drawn in their place.

Needs network access for Google Fonts (Barlow). Chrome: $CHROME, else the Playwright headless
shell, Chrome or Edge in their usual places.
"""
import argparse
import functools
import glob
import http.server
import json
import os
import shutil
import socketserver
import subprocess
import sys
import tempfile
import threading
import urllib.parse

HERE = os.path.dirname(os.path.abspath(__file__))
PLACEHOLDER = (
    b'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 24" shape-rendering="crispEdges">'
    b'<path d="M10 1h2v2h2v2h-2v3h-2z" fill="#3c8a3c"/>'
    b'<rect x="3" y="8" width="15" height="14" rx="6" fill="#d8283c"/>'
    b'<rect x="6" y="11" width="3" height="3" fill="#ff9aa6"/></svg>'
)


def find_chrome():
    if os.environ.get("CHROME"):
        return os.environ["CHROME"]
    local = os.environ.get("LOCALAPPDATA", os.path.expanduser("~/.cache"))
    candidates = sorted(glob.glob(os.path.join(local, "ms-playwright", "chromium_headless_shell-*", "*", "chrome-headless-shell*")))
    candidates += sorted(glob.glob(os.path.expanduser("~/.cache/ms-playwright/chromium_headless_shell-*/*/chrome-headless-shell")))
    candidates += [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        "/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    ]
    for c in candidates:
        if os.path.isfile(c):
            return c
    for name in ("google-chrome", "chromium", "chrome", "msedge"):
        if shutil.which(name):
            return shutil.which(name)
    sys.exit("render.py: no Chrome found; set CHROME to its path")


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        path = urllib.parse.urlparse(self.path).path
        if path.startswith("/_blob/"):
            blob = os.path.basename(path)
            for f in glob.glob(os.path.join(HERE, "blobs", blob + ".*")):
                self.path = "/blobs/" + os.path.basename(f)
                return super().do_GET()
            self.send_response(200)
            self.send_header("Content-Type", "image/svg+xml")
            self.send_header("Content-Length", str(len(PLACEHOLDER)))
            self.end_headers()
            self.wfile.write(PLACEHOLDER)
            return
        return super().do_GET()


def serve(port):
    socketserver.TCPServer.allow_reuse_address = True
    httpd = socketserver.ThreadingTCPServer(("127.0.0.1", port), functools.partial(Handler, directory=HERE))
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd


def shoot(chrome, url, w, h, out, budget):
    with tempfile.TemporaryDirectory() as profile:
        cmd = [chrome, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1",
               "--no-first-run", "--no-default-browser-check", f"--user-data-dir={profile}",
               f"--window-size={w},{h}", f"--virtual-time-budget={budget}", f"--screenshot={out}", url]
        if "headless-shell" in os.path.basename(chrome) or "headless_shell" in os.path.basename(chrome):
            cmd.remove("--headless=new")
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
        if not os.path.isfile(out):
            sys.exit(f"render.py: Chrome wrote no {out}\n{r.stderr[-2000:]}")


def boards(folder):
    with open(os.path.join(HERE, folder, "canvas.json"), encoding="utf-8") as f:
        return json.load(f)["boards"]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("folder", nargs="?", help="artboard folder under docs/design/mockups, e.g. m4")
    ap.add_argument("names", nargs="*", help="artboard names without .dc.html (default: all), optionally ?prop=value")
    ap.add_argument("--out", help="output folder (default <folder>/png)")
    ap.add_argument("--sheet", help="draw the named artboards on one contact sheet PNG instead")
    ap.add_argument("--scale", type=float, default=0.25, help="contact sheet scale (default 0.25)")
    ap.add_argument("--cols", type=int, default=3, help="contact sheet columns (default 3)")
    ap.add_argument("--budget", type=int, default=6000, help="virtual time budget in ms (default 6000)")
    ap.add_argument("--serve", action="store_true", help="only serve the folder for a browser")
    ap.add_argument("--port", type=int, default=8765)
    a = ap.parse_args()

    if a.serve:
        serve(a.port)
        print(f"serving {HERE} at http://127.0.0.1:{a.port}/ (Ctrl+C stops)")
        threading.Event().wait()
    if not a.folder:
        ap.error("give a folder, or --serve")

    all_boards = boards(a.folder)
    picks = a.names or [k[: -len(".dc.html")] for k in all_boards]
    httpd = serve(0)
    port = httpd.server_address[1]
    chrome = find_chrome()
    try:
        if a.sheet:
            cells, x, y, row_h, col = [], 16, 16, 0, 0
            for name in picks:
                base, _, query = name.partition("?")
                b = all_boards[base + ".dc.html"]
                w, h = round(b["w"] * a.scale), round(b["h"] * a.scale)
                cells.append(
                    f'<div style="position:absolute;left:{x}px;top:{y}px;width:{w}px;height:{h + 20}px;overflow:hidden">'
                    f'<div style="font:12px sans-serif;color:#ccc;height:18px">{base}</div>'
                    f'<iframe src="/{a.folder}/{base}.dc.html{"?" + query if query else ""}" '
                    f'style="border:0;width:{b["w"]}px;height:{b["h"]}px;transform:scale({a.scale});transform-origin:0 0"></iframe></div>')
                row_h = max(row_h, h + 28)
                col += 1
                x += w + 16
                if col == a.cols:
                    col, x, y, row_h = 0, 16, y + row_h, 0
            total_w = 16 + max(16 + round(all_boards[p.partition("?")[0] + ".dc.html"]["w"] * a.scale) for p in picks) * min(a.cols, len(picks))
            total_h = y + row_h + 16
            sheet = os.path.join(HERE, a.folder, "_sheet.html")
            with open(sheet, "w", encoding="utf-8") as f:
                f.write('<!doctype html><body style="margin:0;background:#222">' + "".join(cells) + "</body>")
            try:
                shoot(chrome, f"http://127.0.0.1:{port}/{a.folder}/_sheet.html", total_w, total_h, os.path.abspath(a.sheet), a.budget * 2)
            finally:
                os.remove(sheet)
            print(a.sheet)
            return
        out = a.out or os.path.join(HERE, a.folder, "png")
        os.makedirs(out, exist_ok=True)
        for name in picks:
            base, _, query = name.partition("?")
            b = all_boards[base + ".dc.html"]
            suffix = ("-" + query.replace("=", "-").replace("&", "-")) if query else ""
            png = os.path.join(out, base + suffix + ".png")
            shoot(chrome, f"http://127.0.0.1:{port}/{a.folder}/{base}.dc.html{'?' + query if query else ''}", b["w"], b["h"], png, a.budget)
            print(png)
    finally:
        httpd.shutdown()


if __name__ == "__main__":
    main()
