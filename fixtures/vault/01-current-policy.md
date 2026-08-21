---
schema_version: 1
id: policy-current
title: Current deployment policy
type: policy
scope:
  projects: [memoryfs]
  agents: [compiler]
  environments: [production]
provenance:
  source: handbook/deployment
  author: platform-team
  observed_at: "2026-08-20T14:00:00Z"
  source_revision: rev-42
trust: trusted
sensitivity: internal
validity:
  valid_from: "2026-08-20T00:00:00Z"
  valid_until: "2027-08-20T00:00:00Z"
confidence: 0.98
---

# Current deployment policy

Deploy changes through a reviewed pull request.
Consult [[procedure-deploy|the deployment procedure]] before release.

![[architecture.png|Deployment architecture]]
