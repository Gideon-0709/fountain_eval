#!/usr/bin/env python3
"""Check the package boundary and repository documentation without native drivers."""

import json
from pathlib import Path
import re
import subprocess
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True)


def main():
    host = next(line.split(": ", 1)[1] for line in command("rustc", "-vV").splitlines() if line.startswith("host: "))
    metadata = json.loads(command("cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", host))
    members = metadata["workspace_members"]
    packages = metadata["packages"]
    assert len(members) == 1, "The framework must remain one package"
    root = next(package for package in packages if package["id"] == members[0])
    assert root["name"] == "fountain-eval"
    kinds = {kind for target in root["targets"] for kind in target["kind"]}
    assert {"lib", "bin"}.issubset(kinds), "Both library and CLI targets are required"
    native = {"fountain_engine", "fountain_scheme", "fountain_utility", "fountain_bin_ffi"}
    assert not native.intersection(package["name"] for package in packages), "Native dependencies belong to isolated drivers"

    files = command("git", "ls-files", "--cached", "--others", "--exclude-standard", "-z").split("\0")
    documents = {ROOT / file for file in files if file.endswith(".md") and (ROOT / file).is_file()}
    for path in sorted(documents):
        text = path.read_text(encoding="utf-8")
        assert not re.search(r"[\u3400-\u9fff]", text), f"Translate documentation into English: {path}"
        for target in re.findall(r"\[[^\]]*\]\(([^\s)]+)\)", text):
            parsed = urlsplit(target)
            if parsed.scheme or not parsed.path:
                continue
            destination = path.parent / unquote(parsed.path)
            assert destination.exists(), f"Broken document link in {path}: {target}"
    print(f"Checked one framework package, dependency boundary, and {len(documents)} English documents")


if __name__ == "__main__":
    main()
