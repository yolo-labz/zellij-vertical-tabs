#!/usr/bin/env python3
"""Regression oracle for record-demo.py: python docs/check-demo.py CAPTURE_DIR."""
import json
from pathlib import Path
import sys


def check(root):
    expected = [f"Sample {i:02}" for i in range(1, 27)]
    assert (root / "tab-names.txt").read_text().splitlines() == expected
    meta = json.loads((root / "metadata.json").read_text())
    panes = json.loads((root / "panes.json").read_text())
    assert any(p.get("plugin_url") == "file:" + meta["wasm_path"] and not p["exited"] for p in panes)
    checkpoints = json.loads((root / "checkpoints.json").read_text())
    assert [c["tab"] for c in checkpoints] == ["Sample 01", "Sample 13", "Sample 26", "Sample 02"]
    for c in checkpoints:
        assert [t["name"] for t in c["state"]] == expected
        assert [t["name"] for t in c["state"] if t["active"]] == [c["tab"]]
    manifests = list(root.glob("manifest-*.txt"))
    assert manifests, "permission prompt/default layout cannot pass"
    assert all("tabs: 26" in p.read_text() and "26\t" in p.read_text() for p in manifests)
    events = [json.loads(line) for line in (root / "zellij-demo.cast").read_text().splitlines()]
    output = "".join(event[2] for event in events[1:])
    assert "Sample 26" in output and "Sample 13" in output
    assert "↑11" in output and "↓11" in output, "both scroll extremes must render"
    assert "asks permission" not in output
    assert "\ufffd" not in output, "PTY reads must preserve split UTF-8"
    assert float((root / "recording-start.txt").read_text()) < checkpoints[0]["at"]
    print("PASS: exact tabs, built plugin, active transitions, manifests, overflow and UTF-8")


if __name__ == "__main__":
    check(Path(sys.argv[1]))
