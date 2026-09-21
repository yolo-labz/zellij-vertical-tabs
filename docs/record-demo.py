#!/usr/bin/env python3
"""Capture an isolated, synthetic 26-tab zellij session as asciinema v2.

Usage: python docs/record-demo.py /absolute/path/plugin.wasm new-output-directory
No working sessions, real shell prompts or personal configuration are read.
"""

import codecs
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time

wasm = Path(sys.argv[1]).resolve(strict=True)
out = Path(sys.argv[2]).resolve()
out.mkdir(parents=True, exist_ok=False)
zellij = shutil.which("zellij")
assert zellij, "zellij runtime unavailable"
# Unix socket paths have a small limit; the harness TMPDIR can exceed it.
with tempfile.TemporaryDirectory(prefix="zvt-", dir="/tmp") as temporary:
    root = Path(temporary)
    # Allowlist: no inherited ZELLIJ routing, shell init or account environment.
    env = {key: os.environ[key] for key in ("PATH", "LANG") if key in os.environ}
    for key, child in (("HOME", "home"), ("XDG_CONFIG_HOME", "config"),
                       ("XDG_CACHE_HOME", "cache"), ("XDG_DATA_HOME", "data"),
                       ("XDG_RUNTIME_DIR", "run")):
        directory = root / child
        directory.mkdir(mode=0o700)
        env[key] = str(directory)
    env.update(TERM="xterm-256color", SHELL="/bin/sh")
    session = root.name
    env.update(TMPDIR=str(root), ZELLIJ_SOCKET_DIR=str(root / "sockets"))
    config = root / "config.kdl"
    config.write_text('show_startup_tips false\nshow_release_notes false\nsession_serialization false\n'
                      f'session_name "{session}"\n')
    # Pre-authorize ONLY this built plugin in the disposable cache. The plugin
    # deliberately marks itself unselectable, so typing 'y' hits the shell.
    cache = root / "cache" / "zellij"
    cache.mkdir()
    permissions = f'"{wasm}" {{\n ReadApplicationState\n ChangeApplicationState\n}}\n'
    (cache / "permissions.kdl").write_text(permissions)
    (out / "permissions.kdl").write_text(permissions)
    layout = root / "demo.kdl"
    # Only synthetic pane output; no shell startup files, history or accounts.
    command = "printf 'Synthetic demo tab; isolated session\\nNo personal data\\n'; sleep 120"
    text = ('layout {\n default_tab_template {\n pane split_direction="vertical" {\n'
            ' children\n pane size=30 borderless=true {\n'
            f' plugin location="file:{wasm}" {{ timezone "America/Recife"; }}\n'
            ' }\n }\n }\n' + ''.join(
                f' tab name="Sample {i:02}" {{\n pane command="/bin/sh" {{\n'
                f'args "-c" {json.dumps(command)}\n }}\n }}\n'
                for i in range(1, 27)) + '}\n')
    layout.write_text(text)
    (out / "layout.kdl").write_text(text)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 110, 0, 0))
    start = time.monotonic()
    events = []
    decoder = codecs.getincrementaldecoder("utf-8")("replace")
    # 0.44.3 server file-layout resolution needs an existing config directory.
    argv = [zellij, "--config", str(config), "--config-dir", str(root), "--layout", str(layout)]
    process = subprocess.Popen(argv, env=env, cwd=root, stdin=slave,
                               stdout=slave, stderr=slave, start_new_session=True)
    os.close(slave)
    def drain(seconds):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.05)[0]:
                try:
                    data = os.read(master, 65536)
                except OSError:
                    return
                events.append([round(time.monotonic() - start, 6), "o", decoder.decode(data)])
                if b"\x1b[6n" in data:
                    os.write(master, b"\x1b[1;1R")
    try:
        def action(*args):
            return subprocess.check_output([zellij, "--session", session, "action", *args],
                                           env=env, text=True, timeout=5)

        expected = [f"Sample {i:02}" for i in range(1, 27)]
        deadline = time.monotonic() + 15
        names = ""
        while time.monotonic() < deadline:
            drain(0.5)
            try:
                names = action("query-tab-names")
            except subprocess.CalledProcessError:
                continue
            if names.splitlines() == expected:
                break
        if names.splitlines() != expected:
            raise RuntimeError(f"synthetic layout not loaded: {names}")
        (out / "tab-names.txt").write_text(names)
        (out / "live-layout.kdl").write_text(action("dump-layout"))
        # Wait for the real plugin's live sidecar before the presentation starts.
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            manifests = list(root.rglob("tab-manifest.txt"))
            if any("tabs: 26" in p.read_text() for p in manifests):
                break
            drain(0.5)
        else:
            raise RuntimeError("plugin not ready before recording")
        panes = action("list-panes", "--json", "--all")
        if not any(p.get("plugin_url") == f"file:{wasm}" and not p["exited"]
                   for p in json.loads(panes)):
            raise RuntimeError("built plugin missing from live panes")
        (out / "panes.json").write_text(panes)
        (out / "recording-start.txt").write_text(str(time.monotonic() - start) + "\n")
        checkpoints = []
        for tab in ("Sample 01", "Sample 13", "Sample 26", "Sample 02"):
            action("go-to-tab-name", tab)
            drain(1)
            drain(2)
            state = json.loads(action("list-tabs", "--json", "--all"))
            if [t["name"] for t in state if t["active"]] != [tab]:
                raise RuntimeError(f"active tab did not converge: {tab}")
            checkpoints.append({"tab": tab, "at": round(time.monotonic() - start, 6), "state": state})
            drain(2)
        manifests = list(root.rglob("tab-manifest.txt"))
        if not manifests:
            raise RuntimeError("plugin never wrote its live manifest (permissions/render not ready)")
        for i, manifest in enumerate(manifests):
            content = manifest.read_text()
            if "tabs: 26" not in content or "Sample 26" not in content:
                raise RuntimeError("plugin manifest disagrees with tab query")
            (out / f"manifest-{i:02}.txt").write_text(content)
        (out / "checkpoints.json").write_text(json.dumps(checkpoints, indent=2) + "\n")
        subprocess.run([zellij, "kill-session", session], env=env, check=True, timeout=10)
        drain(1)
    finally:
        subprocess.run([zellij, "kill-session", session], env=env, capture_output=True, timeout=10)
        if process.poll() is None:
            process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=3)
        os.close(master)
        header = {"version": 2, "width": 110, "height": 24, "timestamp": int(time.time()),
                  "title": "Isolated zellij vertical sidebar: 26 synthetic tabs", "env": {"TERM": env["TERM"]}}
        (out / "zellij-demo.cast").write_text("\n".join(json.dumps(x) for x in [header, *events]) + "\n")
        (out / "metadata.json").write_text(json.dumps({
            "zellij": subprocess.check_output([zellij, "--version"], text=True).strip(),
            "runtime_path": str(Path(zellij).resolve()),
            "runtime_sha256": hashlib.sha256(Path(zellij).read_bytes()).hexdigest(),
            "launch_argv": argv,
            "capture_helper_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "wasm_path": str(wasm), "wasm_sha256": hashlib.sha256(wasm.read_bytes()).hexdigest(),
            "source_head": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
            "requested_tabs": 26, "dimensions_cells": [110, 24], "playback_speed": 1,
            "audio": None, "fixture": "synthetic tabs; isolated XDG dirs and HOME",
            "duration_seconds": time.monotonic() - start,
        }, indent=2) + "\n")
