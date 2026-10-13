#!/usr/bin/env python3
"""Prove a Cyrius function extraction is BEHAVIOUR-PRESERVING — don't assert it.

Extracting code out of a long function is normally justified with "I only moved lines". In this tree that
claim is not good enough for the functions that matter: `gpu_hdmi_audio_enable` is 497 lines at the centre
of the HDMI-audio mute saga, where a single reordered MMIO write would silently invalidate months of
falsification work and look exactly like a hardware finding.

So verify mechanically. Take the POST-refactor host function, substitute each extracted function's body
back in place of its call, strip comments and blanks, and compare against the PRE-refactor host function.
Identical => the emitted write sequence is unchanged by construction.

Usage:
    # get the pre-image from git (this repo never runs git itself — the operator does):
    #   git show HEAD~1:kernel/core/gpu.cyr > /tmp/pre.cyr
    python3 verify-extraction.py <pre.cyr> <post.cyr> <host_fn> <extracted_fn>[ <extracted_fn>...]

Each extracted function must be called from the host exactly as `name(args);` on its own line.

Proven with it: M8c, 2026-07-24 — gpu_hdmi_stage_attributes + gpu_hdmi_program_infoframes out of
gpu_hdmi_audio_enable, 227 code lines identical.
"""
import re
import sys


def fn_body(path, name):
    """Raw lines of `fn <name>(...) { ... }` — body only, brace lines excluded."""
    out, depth, started = [], 0, False
    for line in open(path):
        if not started:
            if line.startswith(f"fn {name}("):
                started, depth = True, line.count("{") - line.count("}")
            continue
        depth += line.count("{") - line.count("}")
        if depth <= 0:
            break
        out.append(line)
    if not started:
        sys.exit(f"FAIL: fn {name} not found in {path}")
    return out


def code(lines):
    """Executable lines only — comments and blanks emit nothing."""
    return [s for s in (l.strip() for l in lines) if s and not s.startswith("#")]


def main():
    if len(sys.argv) < 5:
        sys.exit(__doc__)
    pre_path, post_path, host = sys.argv[1], sys.argv[2], sys.argv[3]
    extracted = sys.argv[4:]

    pre = code(fn_body(pre_path, host))
    bodies = {name: code(fn_body(post_path, name)) for name in extracted}

    call_re = re.compile(r"^(" + "|".join(re.escape(n) for n in extracted) + r")\s*\(.*\);$")
    rebuilt, substituted = [], 0
    for line in code(fn_body(post_path, host)):
        m = call_re.match(line)
        if m:
            rebuilt.extend(bodies[m.group(1)])
            substituted += 1
        else:
            rebuilt.append(line)

    print(f"pre  {host}: {len(pre)} code lines")
    print(f"post {host} with {substituted} call(s) inlined back: {len(rebuilt)} code lines")
    for name, body in bodies.items():
        print(f"  extracted {name}: {len(body)} code lines")

    if rebuilt == pre:
        print("\nIDENTICAL -- the extraction is behaviour-preserving; the write sequence is unchanged.")
        return 0

    print("\nDIVERGENCE -- the refactor CHANGED the emitted sequence. First differences:")
    shown = 0
    for i, (a, b) in enumerate(zip(pre, rebuilt)):
        if a != b:
            print(f"  line {i}:\n    pre : {a}\n    post: {b}")
            shown += 1
            if shown >= 5:
                break
    if len(pre) != len(rebuilt):
        print(f"  length differs: {len(pre)} vs {len(rebuilt)}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
