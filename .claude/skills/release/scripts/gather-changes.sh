#!/usr/bin/env bash
# List the raw commit/PR history touching an artifact's path(s) since its
# last release tag, for a human (or agent) to draft a changelog entry from.
# Does not attempt to categorize or summarize — this repo's commit messages
# aren't consistently structured enough for reliable auto-classification.
#
# Usage: gather-changes.sh <since-tag> <path-spec>...
#   since-tag: a git tag/ref to diff from, or "" for full history
#   path-spec: one or more paths to scope the log to (e.g. src display)
#
# Run from the repository root.

set -euo pipefail

if [ $# -lt 2 ]; then
  echo "usage: $0 <since-tag|''> <path-spec>..." >&2
  exit 1
fi

since="$1"
shift
paths=("$@")

range=""
if [ -n "$since" ]; then
  range="${since}..HEAD"
fi

echo "## Commits"
git log --oneline ${range:+"$range"} -- "${paths[@]}"

echo
echo "## Merged PR titles"
merge_shas=$(git log --merges ${range:+"$range"} --format='%H' -- "${paths[@]}")
if [ -z "$merge_shas" ]; then
  echo "(none)"
else
  while IFS= read -r sha; do
    subject=$(git log -1 --format='%s' "$sha")
    pr=$(echo "$subject" | grep -oE '#[0-9]+' || true)
    title=$(git log -1 --format='%b' "$sha" | sed -n '1{/^$/d;p}')
    if [ -n "$title" ]; then
      echo "${pr:-(no PR#)}: $title"
    fi
  done <<< "$merge_shas"
fi
