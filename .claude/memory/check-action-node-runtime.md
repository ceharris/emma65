---
name: check-action-node-runtime
description: Before pinning a GitHub Action version, check that the target version's action.yml declares a current Node runtime
metadata:
  type: feedback
---

When adding or editing a `uses:` step, check the target version's `action.yml` for its `runs.using` value. Don't copy whatever version another workflow in the repo uses. Quick check: `gh api repos/<owner>/<repo>/contents/action.yml?ref=<tag> --jq .content | base64 -d | tail -5`. Before a major-version bump, compare the action's inputs against how the step uses them.

**Why:** Node 20 deprecation warnings kept turning up as fixes after the fact (e.g. upload/download-artifact in PR #628, the docker actions in PR #542), when they could have been caught when the workflow was written.
**How to apply:** do this for every workflow change.
