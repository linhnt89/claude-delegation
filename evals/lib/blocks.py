"""Parsing and assertions for managed-block upgrade checks (decisions 032, 033).

A block is the lines between `<!-- delegate:begin <name> -->` and
`<!-- delegate:end <name> -->`, each marker on its own line; markers belong to
the block. Marker lines are byte-exact: the whole line is the marker, with no
surrounding whitespace, and names match [a-z0-9-]+. Anything else, however
marker-like, is ordinary project text.

Used by the S131-S133, P029, S040 and S082 checks. Never hard-codes what the
kit ships: every expectation is read from a template or a file the check built.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

BEGIN_TMPL = "<!-- delegate:begin {name} -->"
END_TMPL = "<!-- delegate:end {name} -->"
NAME_RE = r"[a-z0-9-]+"
BEGIN_RE = re.compile(rf"<!-- delegate:begin ({NAME_RE}) -->")
END_RE = re.compile(rf"<!-- delegate:end ({NAME_RE}) -->")
# Upgrade names a block as <file>#<name>; those lines report block actions, not
# template changes (answer 6: ignore them when judging "not mentioned").
BLOCK_REF_RE = re.compile(r"(AGENTS\.md|CLAUDE\.md|PLAN\.md|evals/jobs\.md|\.delegate\.toml)#[a-z0-9-]+")


def _marker(line: str, kind: str) -> str | None:
    """The block name if the whole line is exactly a begin/end marker, else None."""
    m = (BEGIN_RE if kind == "begin" else END_RE).fullmatch(line)
    return m.group(1) if m else None


def parse(text: str) -> tuple[list[str], dict[str, str]]:
    """Split text into outside lines and {name: block text, markers included}.

    Raises ValueError on anything malformed (unclosed, stray end, nesting,
    mismatched names, duplicates): a block region must be unambiguous.
    """
    outside: list[str] = []
    blocks: dict[str, str] = {}
    current: list[str] | None = None
    name: str | None = None
    for line in text.split("\n"):
        if current is None:
            begin = _marker(line, "begin")
            if begin is not None:
                if begin in blocks:
                    raise ValueError(f"duplicate block {begin!r}")
                name, current = begin, [line]
            elif _marker(line, "end") is not None:
                raise ValueError(f"stray end marker: {line!r}")
            else:
                outside.append(line)
        else:
            current.append(line)
            if _marker(line, "begin") is not None:
                raise ValueError(f"nested begin inside block {name!r}: {line!r}")
            end = _marker(line, "end")
            if end is not None:
                if end != name:
                    raise ValueError(f"end {end!r} closes block {name!r}")
                blocks[name] = "\n".join(current)
                current, name = None, None
    if current is not None:
        raise ValueError(f"unclosed block {name!r}")
    return outside, blocks


def names(text: str) -> list[str]:
    """Block names in file order."""
    _, blocks = parse(text)
    return list(blocks)


def get_block(text: str, name: str) -> str | None:
    """The block with markers, or None when the file has no such block."""
    return parse(text)[1].get(name)


def outside(text: str) -> str:
    """Everything that is not a block, joined back into text."""
    return "\n".join(parse(text)[0])


def outside_allows_append(pre_outside: str, post_outside: str) -> bool:
    """Whether the only outside change is the blank separator before an appended block.

    Answer 3 appends a block as: previous bytes, an optional missing newline, blank
    lines, the block. With the block lines removed, the new outside text is the old
    outside text plus only blank lines.
    """
    return post_outside.startswith(pre_outside) and post_outside[len(pre_outside):].strip() == ""


def mentions(output: str, path: str) -> bool:
    """Whether any non-block line of upgrade output names the file."""
    return any(path in line and not BLOCK_REF_RE.search(line)
               for line in output.splitlines())


def has_action(output: str, ref: str, words: str) -> bool:
    """Whether one output line holds the block ref and one of the words."""
    wants = words.split("|")
    return any(ref in line and any(w in line for w in wants)
               for line in output.splitlines())


def assert_appended(before: bytes, after: bytes, block: bytes) -> None:
    """Answer 3: a new block lands at the end, touching nothing else.

    The file's previous bytes are a prefix of the new file (a missing final
    newline may be added), then only blank lines, then the block, then end of
    file (one final newline allowed).
    """
    head = before if before.endswith(b"\n") or not before else before + b"\n"
    if not after.startswith(head):
        raise AssertionError("upgrade changed a byte outside the new block")
    rest = after[len(head):]
    lines = rest.decode("utf-8").split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    want = block.decode("utf-8").split("\n")
    if want and want[-1] == "":
        want.pop()
    if len(lines) < len(want) or lines[len(lines) - len(want):] != want:
        raise AssertionError("the new block is not at the end of the file")
    for line in lines[:len(lines) - len(want)]:
        if line != "":
            raise AssertionError("more than blank lines were added before the new block")


def kit_before_blocks() -> str:
    """Newest kits.py commit whose AGENTS.md template has no block marker."""
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import kits  # noqa: E402

    for commit in reversed(kits.commits()):
        agents = kits.kit(commit).get("AGENTS.md", b"").decode("utf-8", errors="replace")
        if "delegate:begin" not in agents:
            return commit
    raise SystemExit("CHECK ERROR: every kit in history already has managed blocks")


def old_section(claude_template: str) -> tuple[str, str] | None:
    """Split an old CLAUDE.md template at the lead-role section.

    Returns (prefix, section) where the section runs from the
    `## Claude's role in this project` heading to the end of the template
    (answer 4), or None when the template has no such heading.
    """
    lines = claude_template.split("\n")
    for i, line in enumerate(lines):
        if line == "## Claude's role in this project":
            return "\n".join(lines[:i]), "\n".join(lines[i:])
    return None


def _read(path: str) -> tuple[list[str], bool]:
    text = Path(path).read_text(encoding="utf-8")
    had_nl = text.endswith("\n")
    lines = text.split("\n")
    if had_nl:
        lines.pop()
    return lines, had_nl


def _write(path: str, lines: list[str]) -> None:
    Path(path).write_text("\n".join(lines) + "\n", encoding="utf-8")


def _locate(lines: list[str], name: str) -> tuple[int, int]:
    """(begin, end) line indexes of the block; raises ValueError when absent."""
    begin = end = -1
    for i, line in enumerate(lines):
        if line == BEGIN_TMPL.format(name=name):
            if begin >= 0:
                raise ValueError(f"duplicate block {name!r}")
            begin = i
        elif line == END_TMPL.format(name=name):
            if begin < 0 or end >= 0:
                raise ValueError(f"stray end marker for {name!r}")
            end = i
    if begin < 0 or end < 0:
        raise ValueError(f"no block {name!r}")
    return begin, end


def main() -> None:
    cmd = sys.argv[1]
    if cmd == "get":
        text = Path(sys.argv[2]).read_text(encoding="utf-8")
        block = get_block(text, sys.argv[3])
        if block is None:
            print(f"no block {sys.argv[3]!r} in {sys.argv[2]}", file=sys.stderr)
            raise SystemExit(1)
        sys.stdout.write(block + ("\n" if not block.endswith("\n") else ""))
    elif cmd == "names":
        for name in names(Path(sys.argv[2]).read_text(encoding="utf-8")):
            print(name)
    elif cmd == "outside":
        sys.stdout.write(outside(Path(sys.argv[2]).read_text(encoding="utf-8")))
    elif cmd == "mentions":
        out = Path(sys.argv[2]).read_text(encoding="utf-8", errors="replace")
        raise SystemExit(0 if mentions(out, sys.argv[3]) else 1)
    elif cmd == "kit-before-blocks":
        print(kit_before_blocks())
    elif cmd == "old-section":
        template = Path(sys.argv[2]).read_text(encoding="utf-8")
        found = old_section(template)
        if found is None:
            print("no `## Claude's role in this project` section", file=sys.stderr)
            raise SystemExit(1)
        sys.stdout.write(found[1])
    elif cmd in ("insert-after", "append-inside", "edit-after-begin"):
        path, name = sys.argv[2], sys.argv[3]
        payload = sys.stdin.read().split("\n")
        if payload and payload[-1] == "":
            payload.pop()
        lines, _ = _read(path)
        begin, end = _locate(lines, name)
        at = {"insert-after": end + 1, "append-inside": end,
              "edit-after-begin": begin + 1}[cmd]
        if cmd == "insert-after" and at < len(lines) and lines[at] != "" and payload[:1] != [""]:
            payload = [""] + payload
        lines[at:at] = payload
        _write(path, lines)
    elif cmd == "append-block":
        path, name = sys.argv[2], sys.argv[3]
        payload = sys.stdin.read().split("\n")
        if payload and payload[-1] == "":
            payload.pop()
        lines, _ = _read(path)
        if name in names("\n".join(lines) + "\n"):
            raise SystemExit(f"CHECK ERROR: block {name!r} already exists")
        if lines == [""]:
            lines = []
        elif lines[-1] != "":
            lines.append("")
        lines += [BEGIN_TMPL.format(name=name)] + payload + [END_TMPL.format(name=name)]
        _write(path, lines)
    elif cmd == "remove-block":
        path, name = sys.argv[2], sys.argv[3]
        lines, _ = _read(path)
        begin, end = _locate(lines, name)
        del lines[begin:end + 1]
        _write(path, lines)
    else:
        raise SystemExit(f"unknown command {cmd}")


if __name__ == "__main__":
    main()
