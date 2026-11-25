#!/bin/sh
# Decides whether the nightly launcher build should run.
#
# The record of "the last nightly" is the newest *successful* run of the nightly
# workflow on the same branch, and its head commit. A failed or cancelled night
# never becomes the record, so the next run rebuilds that commit; a skipped
# night records the commit it skipped on, which is already the last built one.
set -eu

workflow=${GATE_WORKFLOW:-launcher-nightly.yml}
branch=${GATE_BRANCH:-${GITHUB_REF_NAME:-master}}
head=${GATE_HEAD_SHA:-${GITHUB_SHA:-}}
force=${GATE_FORCE:-false}

if [ -z "$head" ]; then
  echo "nightly gate: set GATE_HEAD_SHA or GITHUB_SHA to the commit to build" >&2
  exit 1
fi

last=$(gh run list --workflow "$workflow" --branch "$branch" --status success --limit 1 --json headSha --jq '.[0].headSha // ""')

if [ "$force" = "true" ]; then
  should_build=true
  reason="forced by workflow_dispatch"
elif [ -z "$last" ]; then
  should_build=true
  reason="no successful nightly run on $branch yet"
elif [ "$last" = "$head" ]; then
  should_build=false
  reason="last successful nightly already built this commit"
else
  should_build=true
  reason="commits landed since the last successful nightly"
fi

if [ -n "${GITHUB_OUTPUT:-}" ]; then
  {
    echo "should_build=$should_build"
    echo "last_built_sha=$last"
  } >>"$GITHUB_OUTPUT"
fi

report="nightly gate: build=$should_build
- reason: $reason
- head: $head
- last successful nightly built: ${last:-none}
- branch: $branch"

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  printf '%s\n' "$report" >>"$GITHUB_STEP_SUMMARY"
fi
printf '%s\n' "$report"
