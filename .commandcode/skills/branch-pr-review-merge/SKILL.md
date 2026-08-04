---
name: branch-pr-review-merge
description: Create a branch from the current working changes, commit and push them, open a GitHub pull request, review the diff, run the repository checks, and merge the PR into master only when review and validation pass. Use when asked to branch, create a PR, review, and merge changes.
argument-hint: "[branch-name]"
disable-model-invocation: true
metadata:
  author: SergioJuniorCE
  version: "1.0"
---

# Branch, PR, review, and merge

Perform the complete GitHub workflow for the current repository. Treat merge as a gated operation, not an assumption.

## Safety rules

- Never use `git reset --hard`, `git clean`, force-push, or destructive deletion to hide work.
- Never overwrite unrelated working-tree changes.
- Never skip hooks or checks with `--no-verify`.
- Never merge if validation fails, review findings remain unresolved, the PR is not mergeable, or required status checks are incomplete.
- Do not push or merge until the current changes have been inspected and the branch/commit scope is clear.
- Stop and report the blocker when authentication, permissions, conflicts, missing checks, or remote configuration prevent the workflow.
- Ask for confirmation immediately before merging if the invocation did not explicitly request merging. If the invocation explicitly says to merge, proceed after all gates pass.

## Workflow

### 1. Inspect repository state

Run:

```sh
git status --short --branch
git diff --stat
git diff --cached --stat
git remote -v
gh auth status
gh repo view --json nameWithOwner,defaultBranchRef
```

Determine:

- The current branch and whether it is already a feature branch.
- Whether changes are unstaged, staged, untracked, or already committed.
- The repository owner/name and default branch.
- The validation commands from `package.json`, workspace manifests, CI workflows, and project instructions.

If there are unrelated changes, preserve them and ask which files belong in the PR. Do not stage them accidentally.

### 2. Choose and create the branch

Use the supplied `[branch-name]` when valid. Otherwise derive a concise branch name from the change, using the repository convention when one exists. Prefix feature branches with `agent/` unless the repository convention says otherwise.

If currently on `master` or another shared branch:

```sh
git switch -c <branch-name>
```

If already on a suitable feature branch, keep it and report the branch name. Do not create a second branch from a branch that already contains the current changes.

### 3. Review the pending changes before committing

Inspect the complete relevant diff:

```sh
git diff
 git diff --cached
```

Check for:

- Secrets, credentials, tokens, private keys, generated artifacts, and unrelated files.
- Correct scope and tests for the requested change.
- Security, error handling, compatibility, and platform concerns.
- Formatting, compiler warnings, and lint issues.

Fix issues only when they are directly related and safe to resolve. Otherwise stop and report them before creating the PR.

### 4. Commit and push

Stage only the intended files:

```sh
git add <specific-files>
```

Create a focused commit using a HEREDOC. The commit message must end with the required trailer:

```sh
git commit -F - <<'EOF'
<imperative change summary>

Co-authored-by: CommandCodeBot <noreply@commandcode.ai>
EOF
```

Push the branch and set its upstream:

```sh
git push -u origin <branch-name>
```

Do not amend an existing commit unless explicitly requested.

### 5. Open the pull request

Create a non-draft PR targeting `master` unless the repository’s default branch or explicit request says otherwise:

```sh
gh pr create --base master --head <branch-name> --title "<concise title>" --body-file <body-file>
```

Use a temporary body file under `$COMMANDCODE_SCRATCHPAD` when needed. Include:

- What changed.
- Why it changed.
- Validation commands and results.
- Any known limitations or follow-up work.

Capture the PR number and URL.

### 6. Review the PR diff

Review the actual PR, not only the local diff:

```sh
gh pr diff <pr-number>
gh pr view <pr-number> --json title,body,baseRefName,headRefName,mergeStateStatus,statusCheckRollup,isDraft
```

Check every changed file for correctness, regressions, security issues, missing tests, and unnecessary complexity. If findings exist:

1. Fix them on the branch.
2. Run the affected checks again.
3. Commit and push the fixes.
4. Re-review the updated PR.

Do not approve or merge a PR with unresolved findings. If repository review permissions are available, submit a review with `gh pr review`; otherwise record the review result in the final response.

### 7. Run validation and inspect CI

Run the repository’s relevant local checks, including the commands discovered in step 1. At minimum run the affected package’s tests, typecheck, lint, and build where available. Fix all errors and warnings.

Then inspect remote checks:

```sh
gh pr checks <pr-number> --watch
```

If `--watch` is unavailable, poll with `gh pr checks <pr-number>` without using a shell sleep loop. Do not merge while checks are pending or failing.

### 8. Merge only after every gate passes

Before merging, verify:

- The PR targets `master`.
- The PR is not a draft.
- The working tree has no unintended changes.
- The final diff has been reviewed.
- Local validation passes without unresolved warnings.
- Required CI checks pass.
- The PR is mergeable and has no unresolved review requests or conflicts.

If merge was explicitly requested and all gates pass, use the repository’s preferred merge method. Prefer a merge commit unless repository policy says otherwise:

```sh
gh pr merge <pr-number> --merge --delete-branch=false
```

Do not delete the branch automatically unless explicitly requested. Verify the result:

```sh
gh pr view <pr-number> --json state,mergedAt,mergeCommit
 git status --short --branch
```

If confirmation is required by local policy or merge was not explicitly requested, stop after the PR review and report the exact merge command that is ready.

## Final report

Report concisely:

- Branch name.
- Commit hash.
- PR number and URL.
- Review result.
- Validation commands and outcomes.
- Merge commit/state, or the exact blocker preventing merge.
