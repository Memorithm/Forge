# AX-inspired isolated candidate bootstrap

Forge adopts the useful task/workspace/isolation concepts from Google AX as Memorithm-owned Rust contracts. Google AX is not a dependency.

## Forge-specific rule

Generated or mutated candidate code is untrusted by default. `timeout` and `rlimit` remain defense in depth and must be classified as `SupervisedProcess`, never as a security sandbox.

Target execution envelope:

`PROPOSE/MUTATE -> COMPILE -> VERIFY -> MEASURE -> SELECT`

with each executable candidate carrying:

- exact source/candidate identity;
- exact toolchain/environment identity;
- explicit CPU/RAM/wall-clock/file budgets;
- network policy, normally default-deny;
- minimum isolation class;
- immutable verification and measurement evidence.

## Phases

- AXF-1: explicit isolation class in forge-core (current slice).
- AXF-2: attach isolation requirement and resource envelope to candidate execution requests.
- AXF-3: make remote worker advertise enforceable isolation/network/resource capabilities.
- AXF-4: fail closed when a worker cannot satisfy the candidate envelope.
- AXF-5: integrate container-or-stronger backend via scirust-hub/RemoteOps without moving search semantics out of Forge.
- AXF-6: hostile-candidate qualification campaign: filesystem, network, fork/process, memory/file exhaustion and timeout escape attempts.
- AXF-7: task-scoped workload identity and authenticated worker evidence.
- AXF-8: optional suspend/resume only for side-effect-safe evaluation phases.

Forge retains independent correctness verification. Isolation evidence cannot make an incorrect candidate survive.
