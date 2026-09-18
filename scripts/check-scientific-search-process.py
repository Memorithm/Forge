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

# Version identity is explicit on the actual executable protocol.
for strategy, version in (("adaptive-tpe", "forge-finite-tpe/v1"),
                          ("adaptive-tpe-early", "forge-finite-tpe-early/v1"),
                          ("adaptive-gp", "forge-finite-gp/v1")):
    adaptive = copy.deepcopy(fixture)
    adaptive["spec"]["strategy"] = strategy
    adaptive["spec"]["generator_version"] = version
    adaptive["spec"]["manifest"]["external_domain"]["objectives"] = adaptive["spec"]["manifest"]["external_domain"]["objectives"][:1]
    adaptive["spec"]["objective_units"] = adaptive["spec"]["objective_units"][:1]
    response = call(encode(adaptive))
    assert response == call(encode(adaptive))
    assert response["snapshot"]["candidates"][0]["proposal"]["generator_version"] == version
    adaptive["spec"]["generator_version"] = "forge-finite-search/v1"
    call(encode(adaptive), False)
print("Adaptive TPE and SciRust GP: explicit version identity and replay passed")

protocol = "forge-scientific-session/v1"
opened = {"protocol": protocol, "action": {"op": "open", "spec": fixture["spec"], "checkpoint": None}}
command = {"protocol": protocol, "action": {"op": "command", "spec_sha256": first["checkpoint"]["spec_sha256"],
           "expected_sequence": 0, "command": fixture["command"]}}
inspect = {"protocol": protocol, "action": {"op": "inspect", "spec_sha256": first["checkpoint"]["spec_sha256"],
           "expected_sequence": 1}}
raw = b"\n".join(map(encode, (opened, command, inspect))) + b"\n"
result = subprocess.run([str(args.worker.resolve()), "--session"], input=raw, capture_output=True, timeout=30)
assert result.returncode == 0, result.stderr
responses = [json.loads(line) for line in result.stdout.splitlines()]
assert len(responses) == 3
assert all(r["protocol"] == protocol for r in responses)
assert responses[1]["result"]["receipt"] == first["snapshot"]["receipts"][-1]
assert responses[2]["result"]["response"] == first
for bad in (encode(opened), encode(opened) + b"\n" + encode(opened) + b"\n",
            encode(opened) + b"\n" + encode(dict(command, action=dict(command["action"], expected_sequence=1))) + b"\n"):
    result = subprocess.run([str(args.worker.resolve()), "--session"], input=bad, capture_output=True, timeout=30)
    assert result.returncode == 21
print("Persistent session: exact replay projection, receipt identity, framing and stale sequence rejection passed")

inspect_empty = dict(inspect, action=dict(inspect["action"], expected_sequence=0))
limit_frames = encode(opened) + b"\n" + (encode(inspect_empty) + b"\n") * 4097
for extra, code in ((b"", 0), (encode(inspect_empty) + b"\n", 21)):
    result = subprocess.run([str(args.worker.resolve()), "--session"], input=limit_frames + extra,
                            capture_output=True, timeout=30)
    assert result.returncode == code, result.stderr
    assert len(result.stdout.splitlines()) == 4098
print("Session frame bound: exactly 4098 replies closes cleanly; a 4099th frame is rejected")
