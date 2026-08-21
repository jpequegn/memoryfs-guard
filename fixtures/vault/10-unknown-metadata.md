---
schema_version: 1
id: forward-compatible-note
title: Forward-compatible metadata
type: decision
scope:
  projects: [memoryfs]
  agents: [compiler]
  environments: [development]
  teams: [platform]
provenance:
  source: architecture-review
  observed_at: "2026-08-21T09:00:00Z"
  collector: fixture-generator
trust: reviewed
sensitivity: internal
future_extension:
  mode: experimental
  weight: 7
---

# Forward-compatible metadata

Unknown metadata must survive serialization and migration.
