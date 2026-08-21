---
schema_version: 1
id: deploy-window-b
title: Weekend deployment window
type: instruction
scope:
  projects: [memoryfs]
  agents: [release-agent]
  environments: [production]
provenance:
  source: release-guide-b
  observed_at: "2026-07-02T09:00:00Z"
trust: reviewed
sensitivity: internal
conflicts_with: [deploy-window-a]
---

# Weekend deployment window

Deploy only on Saturday between 10:00 and 12:00 UTC.
