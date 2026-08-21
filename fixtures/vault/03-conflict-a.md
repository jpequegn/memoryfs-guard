---
schema_version: 1
id: deploy-window-a
title: Weekday deployment window
type: instruction
scope:
  projects: [memoryfs]
  agents: [release-agent]
  environments: [production]
provenance:
  source: release-guide-a
  observed_at: "2026-07-01T09:00:00Z"
trust: reviewed
sensitivity: internal
conflicts_with: [deploy-window-b]
---

# Weekday deployment window

Deploy only between 09:00 and 17:00 UTC on weekdays.
