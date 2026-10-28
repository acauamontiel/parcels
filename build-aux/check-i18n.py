#!/usr/bin/env python3
"""Checks that every translatable string reaches the translation template.

- Every source file with translatable strings must be listed in po/POTFILES.in.
- Every gettext("…") literal in Rust code must be extracted by xgettext.
  xgettext's Rust parser can miss strings inside some closures; when that
  happens, move the string into a plain function.

Usage: check-i18n.py SOURCE_ROOT
"""
import pathlib
import re
import subprocess
import sys
import tempfile

root = pathlib.Path(sys.argv[1])
potfiles = [
    line.strip()
    for line in (root / "po" / "POTFILES.in").read_text().splitlines()
    if line.strip() and not line.startswith("#")
]
errors = []

patterns = {".rs": re.compile(r'gettext\(\s*"'), ".blp": re.compile(r'_\("')}  # also matches ngettext
for path in sorted((root / "src").rglob("*.rs")) + sorted((root / "data").rglob("*.blp")):
    relative = path.relative_to(root).as_posix()
    if patterns[path.suffix].search(path.read_text()) and relative not in potfiles:
        errors.append(f"{relative} has translatable strings but is not in po/POTFILES.in")

with tempfile.TemporaryDirectory() as tmp:
    pot = pathlib.Path(tmp) / "check.pot"
    subprocess.run(
        ["xgettext", "--from-code=UTF-8", "--keyword=_", "--keyword=gettext", "--keyword=ngettext:1,2",
         f"--directory={root}", f"--files-from={root / 'po' / 'POTFILES.in'}",
         f"--output={pot}"],
        check=True, stderr=subprocess.DEVNULL,
    )
    text = pot.read_text()

msgids = set()
for match in re.finditer(r'^msgid ((?:"(?:[^"\\]|\\.)*"\n?)+)', text, re.M):
    msgids.add("".join(re.findall(r'"((?:[^"\\]|\\.)*)"', match.group(1))))

for name in potfiles:
    if not name.endswith(".rs"):
        continue
    source = (root / name).read_text()
    # For ngettext, this checks the singular, which is the msgid
    for match in re.finditer(r'gettext\(\s*"((?:[^"\\]|\\.)*)"', source):
        if match.group(1) not in msgids:
            line = source.count("\n", 0, match.start()) + 1
            errors.append(f"{name}:{line}: xgettext does not extract {match.group(1)!r}")

for error in errors:
    print(error)
sys.exit(1 if errors else 0)
