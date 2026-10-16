"""Read tracked public Markdown/JSON; report locations, never matched values.

This is a narrow publication hygiene guard, not a general secret detector.
Run from the repository root. No home directories, untracked files, network,
or application/dependency code are read or executed.
"""

import json
import pathlib
import re
import subprocess
import sys


HOME_PATH = re.compile(r"/Users/([^/\s\"'<>`\\]+)(?=/|[\s\"'`]|$)")
# Literal examples and the GitHub runner account, not arbitrary username patterns.
EXAMPLE_ACCOUNTS = frozenset({"runner", "example", "test-user"})
SERIAL = re.compile(r"\bRF[A-Z0-9]{8,16}\b")
DEVICE_CONTEXT = re.compile(r"(?i)(?:\bserial\b|\badb\b|\bdevice(?:_id|Id)?\b)")
INDEX_FIELD = re.compile(r'"(?:before|after)_base64"\s*:')


def inspect_text(text):
    """Return only (one-based line, static category); do not return values."""
    findings = []
    for line, value in enumerate(text.splitlines(), 1):
        if any(m[1] not in EXAMPLE_ACCOUNTS for m in HOME_PATH.finditer(value)):
            findings.append((line, "personal-home-path"))
        if DEVICE_CONTEXT.search(value) and SERIAL.search(value):
            findings.append((line, "physical-device-serial"))
        if INDEX_FIELD.search(value):
            findings.append((line, "embedded-raw-index"))
    # Cover other *_base64 field names within a JSON raw_index object too.
    try:
        document = json.loads(text)
    except (ValueError, RecursionError):
        document = None  # Markdown, including fenced examples, is checked above.

    def walk(value, in_index=False):
        if isinstance(value, dict):
            for key, child in value.items():
                inside = in_index or key == "raw_index"
                if in_index and key.endswith("_base64"):
                    pattern = re.compile(re.escape(json.dumps(key)) + r"\s*:")
                    locations = [i for i, row in enumerate(text.splitlines(), 1) if pattern.search(row)]
                    findings.extend((i, "embedded-raw-index") for i in locations or [1])
                walk(child, inside)
        elif isinstance(value, list):
            for child in value:
                walk(child, in_index)

    try:
        walk(document)
    except RecursionError:
        findings.append((1, "document-too-deep-to-inspect"))
    return sorted(set(findings))


def main():
    names = subprocess.check_output(["git", "ls-files", "-z"]).decode().split("\0")
    failed = False
    for name in names:
        if not name.endswith((".md", ".json")):
            continue
        path = pathlib.Path(name)
        if path.is_symlink():
            findings = [(1, "document-symlink-not-inspected")]
        else:
            findings = inspect_text(path.read_text(encoding="utf-8"))
        for line, category in findings:
            print(json.dumps({"path": name, "line": line, "category": category}))
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
