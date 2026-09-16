#!/usr/bin/env python3
"""Actual bounded Forge process interchange; no candidate evaluator is mocked."""
import argparse
import copy
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--worker", type=Path, required=True)
args = parser.parse_args()
fixture = json.loads((Path(__file__).resolve().parents[1] / "forge-bridge/examples/scientific-search-request.json").read_text())


def call(raw, success=True):
    result = subprocess.run([str(args.worker.resolve())], input=raw, capture_output=True, timeout=30, check=False)
    assert result.returncode == (0 if success else 21), result.stderr[-1000:]
    if success:
        return json.loads(result.stdout)
    assert not result.stdout


encode = lambda x: json.dumps(x, separators=(",", ":")).encode()
first = call(encode(fixture))
assert first == call(encode(fixture))
assert first["checkpoint"]["commands"] == [fixture["command"]]
assert first["snapshot"]["candidates"][0]["proposal"]["ordinal"] == 0
assert first["snapshot"]["candidates"][0]["metrics"] is None
resumed = dict(fixture, checkpoint=first["checkpoint"])
duplicate = call(encode(resumed))
assert duplicate["snapshot"]["receipts"][-1] == {"status": "duplicate", "original_index": 0}
assert duplicate["snapshot"]["attempts"] == 0
assert call(encode(dict(resumed, command=None))) == first
changed = copy.deepcopy(resumed)
changed["spec"]["seed"] = "0"
call(encode(changed), False)
unknown = copy.deepcopy(fixture)
unknown["spec"]["manifest"]["external_domain"]["upstream"]["unknown"] = 1
call(encode(unknown), False)
forbidden_baseline = copy.deepcopy(fixture)
forbidden_baseline["spec"]["forbidden_combinations"] = [{"implementation": "reference"}]
call(encode(forbidden_baseline), False)
raw = encode(fixture)
for bad in [b'{"spec":{},' + raw[1:], raw.replace(b'"seed":"18446744073709551615"', b'"seed":18446744073709551616'),
            raw.replace(b'"commit_id":', b'"commit_id":"duplicate", "commit_id":', 1), b" " * (4 * 1024 * 1024 + 1)]:
    call(bad, False)
print("Forge process: deterministic replay, exact duplicates, nested closed schema, u64 seed and input bounds passed")
