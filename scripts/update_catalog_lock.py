#!/usr/bin/env python3
"""Refresh the unreleased candidate seal after intentional I/T changes."""
import json
import subprocess
from pathlib import Path
root=Path(__file__).resolve().parent.parent
catalog=json.loads(subprocess.check_output(["cargo","run","--quiet","--","list"],cwd=root))
locked={"format_version":1,"publication_status":"internal-draft","interfaces":catalog["interfaces"],"tests":[t["identity"] for t in catalog["test_versions"]]}
(root/"specs/catalog-lock.json").write_text(json.dumps(locked,indent=2)+"\n")
