import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

const script = path.resolve(import.meta.dirname, "nightly-gate.sh");

// Stands in for `gh run list --json headSha --jq '.[0].headSha // ""'`, which
// prints the last built commit or an empty line.
function stubGh({ lastSha = "", fail = false }) {
  const root = mkdtempSync(path.join(os.tmpdir(), "nightly-gate-"));
  const bin = path.join(root, "bin");
  mkdirSync(bin);
  const calls = path.join(root, "calls");
  writeFileSync(calls, "");
  writeFileSync(
    path.join(bin, "gh"),
    fail ? `#!/bin/sh\necho "gh: HTTP 502" >&2\nexit 1\n` : `#!/bin/sh\nprintf '%s\\n' "$*" >>"${calls.replaceAll("\\", "/")}"\nprintf '%s\\n' "${lastSha}"\n`,
  );
  chmodSync(path.join(bin, "gh"), 0o755);
  return { bin, calls };
}

function gate({ head = "head-sha", ...stub } = {}) {
  const { bin, calls } = stubGh(stub);
  const outputs = path.join(path.dirname(bin), "output");
  const summary = path.join(path.dirname(bin), "summary");
  writeFileSync(outputs, "");
  writeFileSync(summary, "");
  execFileSync("sh", [script], {
    env: {
      PATH: `${bin}${path.delimiter}${process.env.PATH}`,
      GATE_HEAD_SHA: head,
      GATE_FORCE: stub.force ?? "",
      GATE_BRANCH: "master",
      GITHUB_OUTPUT: outputs,
      GITHUB_STEP_SUMMARY: summary,
    },
    encoding: "utf8",
  });
  return {
    queried: readFileSync(calls, "utf8").trim(),
    values: Object.fromEntries(
      readFileSync(outputs, "utf8")
        .split("\n")
        .filter((line) => line.includes("="))
        .map((line) => [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1)]),
    ),
    summary: readFileSync(summary, "utf8"),
  };
}

test("builds when no nightly has ever succeeded", () => {
  const result = gate({ lastSha: "" });
  assert.equal(result.values.should_build, "true");
  assert.match(result.summary, /no successful nightly run on master yet/);
});

test("builds when commits landed since the last successful nightly", () => {
  const result = gate({ head: "new-sha", lastSha: "old-sha" });
  assert.equal(result.values.should_build, "true");
  assert.equal(result.values.last_built_sha, "old-sha");
  assert.match(result.summary, /commits landed since the last successful nightly/);
});

test("skips when the head is already the last built commit", () => {
  const result = gate({ head: "same-sha", lastSha: "same-sha" });
  assert.equal(result.values.should_build, "false");
  assert.match(result.summary, /build=false/);
});

test("reads only successful runs of this workflow on this branch", () => {
  const result = gate({ lastSha: "old-sha" });
  assert.match(result.queried, /--workflow launcher-nightly\.yml/);
  assert.match(result.queried, /--branch master/);
  assert.match(result.queried, /--status success/g);
  assert.match(result.queried, /--limit 1/);
});

test("force rebuilds an unchanged head", () => {
  const result = gate({ head: "same-sha", lastSha: "same-sha", force: "true" });
  assert.equal(result.values.should_build, "true");
  assert.match(result.summary, /forced by workflow_dispatch/);
});

test("fails instead of skipping when the run history cannot be read", () => {
  assert.throws(() => gate({ fail: true }));
});
