#!/usr/bin/env python3
"""check-dep-tags.py — does every tag a repo DECLARES actually exist upstream?

⛔ WHY THIS EXISTS. Three CI failures in one day (2026-08-02), all the same shape:

  1. aethersafha -> mehman 1.0.0   the tag EXISTED but its content predated a rename in kavach,
                                   so CI compiled a `backend_name` that no longer existed.
  2. crab -> setu 0.7.1            the fix the consumer needed lived in an uncut 0.7.2.
  3. crab -> dhancha 0.9.3         a real, fully-documented local cut that was never pushed.

Every one was invisible locally and obvious in CI, for one reason: a `path = "../X"` override in
`cyrius.cyml` WINS over the tag. So the developer compiles their working tree while CI compiles the
declared graph, and the two are different programs. A path override does not merely pin a version —
**it disables the tag as a test**, and CI becomes the first honest build.

⚠ A VERSION-vs-tag comparison does NOT catch this. In case 1 the consumer declared 1.0.0, the
dependency's VERSION file said 1.0.0, and a drift sweep reported a clean match — while the SOURCE had
moved underneath both. A version number is evidence about tag contents only if every source change
bumps it. This checks EXISTENCE upstream, and separately flags the overrides that hide problems.

Usage:
    python3 scripts/check-dep-tags.py [repo-dir ...]      # default: current directory

Exit 0 = every declared tag exists upstream. Exit 1 = at least one does not (do not push the
consumer yet — push its dependency first). Network failures are reported as unknown, never as OK.
"""
import json
import os
import re
import sys
import urllib.error
import urllib.request

OWNER = os.environ.get("AGNOS_GH_OWNER", "MacCracken")
DEP_RE = re.compile(r'\[deps\.([A-Za-z0-9_-]+)\]((?:\n(?!\[).*)*)')


def upstream_tags(repo):
    """Every tag name published for `repo`, or None if the question could not be answered."""
    url = f"https://api.github.com/repos/{OWNER}/{repo}/tags?per_page=100"
    req = urllib.request.Request(url, headers={"User-Agent": "agnos-check-dep-tags"})
    try:
        with urllib.request.urlopen(req, timeout=20) as r:
            data = json.load(r)
        if not isinstance(data, list):
            return None
        return {t["name"] for t in data}
    except (urllib.error.URLError, json.JSONDecodeError, KeyError, TimeoutError):
        return None


def check(repo_dir):
    manifest = os.path.join(repo_dir, "cyrius.cyml")
    if not os.path.exists(manifest):
        print(f"  no cyrius.cyml in {repo_dir} — skipped")
        return 0

    name = os.path.basename(os.path.abspath(repo_dir))
    version = "?"
    vf = os.path.join(repo_dir, "VERSION")
    if os.path.exists(vf):
        version = open(vf).read().strip()
    print(f"\n=== {name} {version} ===")

    txt = open(manifest).read()
    missing, unknown, overrides = [], [], []

    for dep, body in DEP_RE.findall(txt):
        tag_m = re.search(r'^\s*tag\s*=\s*"([^"]+)"', body, re.M)
        path_m = re.search(r'^\s*path\s*=\s*"([^"]+)"', body, re.M)
        if path_m:
            overrides.append(dep)
        if not tag_m:
            continue
        tag = tag_m.group(1)
        tags = upstream_tags(dep)
        if tags is None:
            unknown.append((dep, tag))
            state = "?  could not reach GitHub"
        elif tag in tags:
            state = "OK"
        else:
            missing.append((dep, tag))
            newest = sorted(tags)[-1] if tags else "(none)"
            state = f"MISSING upstream (newest there: {newest})"
        flag = " [path override]" if path_m else ""
        print(f"  {dep:<12} {tag:<10} {state}{flag}")

    if overrides:
        print()
        print("  ⚠ path overrides present: " + ", ".join(overrides))
        print("    These WIN over the tag, so local builds do not exercise the declared graph.")
        print("    Fine while developing against a sibling checkout; remove before pushing.")

    if missing:
        print()
        for dep, tag in missing:
            print(f"  ⛔ {dep} {tag} is NOT published — push/tag {dep} BEFORE this repo.")
        print("    Push in dependency order, or the consumer's CI resolves a tag that does not exist.")
        return 1

    if unknown:
        print()
        print("  ⚠ could not verify: " + ", ".join(f"{d} {t}" for d, t in unknown))
        print("    Treated as UNKNOWN, not OK — re-run when the network is available.")
        return 1

    print("  all declared tags exist upstream")
    return 0


def main():
    targets = sys.argv[1:] or ["."]
    rc = 0
    for t in targets:
        rc |= check(t)
    return rc


if __name__ == "__main__":
    sys.exit(main())
