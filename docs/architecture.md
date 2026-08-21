# Architecture

## Components

`memoryfs-core` is the portable trust boundary. It owns note contracts, parsing, migration, linting, context compilation, deterministic digests, resource limits, and evaluation. It performs no filesystem, network, or Git operations.

`memoryfs-git` is the native repository adapter. It reads commit trees and staged blobs, computes semantic diffs, rebuilds indexes, and creates detached proposals. Only an explicit `apply_proposal` call writes a working-tree file, after checking the expected base digest and vault-relative path.

`memoryfs-wasm` exposes parse, lint, graph, and compile functions as canonical JSON. Native parity fixtures call the same export implementations and compare them with direct core results.

`memoryfs-cli` supplies filesystem orchestration, commands, status gates, and report generation. `web/` is a static, same-origin interface over generated WASM bindings.

## Data Flow

1. A native adapter or browser file picker supplies Markdown source.
2. The parser migrates front matter in memory and preserves the original source.
3. The linter emits findings with stable codes, source locations, suppression state, and a digest.
4. The compiler rejects ineligible memory before ranking.
5. Eligible excerpts are selected under a fixed token budget.
6. Every candidate receives a decision, reason, status, score, and location.
7. A content-addressed receipt binds the task, policy, vault, decisions, and excerpts.

## Determinism

Inputs are sorted by stable path and ID before hashing or selection. Evaluation time is explicit. Vector-fixture ranking uses a fixed signed-hash projection rather than a downloaded model. Receipts use canonical struct serialization and SHA-256. Same inputs, policy version, budget, and timestamp produce the same output.

## Authority Before Relevance

Scope, sensitivity, trust, temporal validity, unresolved conflicts, and error-level lint findings are hard gates. Ranking cannot restore a rejected candidate. This ordering prevents a highly relevant restricted or poisoned note from entering a pack.
