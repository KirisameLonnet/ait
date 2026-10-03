#!/usr/bin/env python3
"""Offline GitHub checkout fixture; input and responses live in the isolated test directory."""

import json
import pathlib
import sys

root = pathlib.Path(__file__).parent
if sys.argv[1] == "repo":
    print(json.dumps({"owner": {"login": "base"}, "name": "repository"}))
elif sys.argv[1] == "pr":
    print(json.dumps({"headRefName": "topic"}))
else:
    print((root / "pull-request.json").read_text())
