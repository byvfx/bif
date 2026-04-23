# Release Ship Workflow

Use this workflow when executing a release-closeout handoff or preparing a taggable ship state in this repo.

If you edit this shared workflow, re-run `scripts/sync-agent-config.ps1` so the Claude and Kilo wrappers stay aligned.

## 1. Read The Execution Context

Start from the live repo docs before changing code:

- `agents/base/PROJECT.md`
- `agents/base/HANDOFF_CONTRACT.md`
- the current handoff file in `docs/agent-handoffs/`
- any referenced workflow docs
- `CLAUDE.md`
- `SESSION_HANDOFF.md`

## 2. Compare The Handoff To Reality

Before implementation:

- verify the referenced files actually exist
- verify the branch and working tree state
- verify the checks in the handoff still match the repo's current environment needs

If the handoff and repo disagree, follow the repo state and report the mismatch explicitly.

## 3. Implement In Focused Batches

- Keep the change sequence aligned with the handoff's intended commit boundaries.
- Do not collapse unrelated cleanup into the feature commits.
- If extra fixes are required to make the repo green, isolate them in their own focused commit.

## 4. Run Release Validation

Run the handoff's listed checks, plus any repo-required environment setup from `CLAUDE.md`.

At minimum, confirm:

- format passes
- clippy passes
- the requested builds pass
- the requested test suites pass

If a check fails because the handoff is stale, fix the underlying issue or report the blocker before tagging.

## 5. Update Release Docs

Keep release-facing docs accurate where relevant:

- `CHANGELOG.md`
- `CLAUDE.md`
- `README.md`
- `MILESTONES.md`
- `SESSION_HANDOFF.md`
- `devlog/`
- the handoff file itself if it must reflect resolved mismatches

## 6. Prepare The Ship History

- Stage only the files for the current commit.
- Use concise conventional commit messages.
- Keep the final history easy to read from `git log --oneline`.

## 7. Tag And Publish

When the working tree and checks are green:

- create the annotated release tag requested by the handoff
- push the branch and tag if the task calls for it
- verify any release-adjacent automation named in the handoff (for example Pages rebuilds)

## 8. Report The Outcome

Report:

- checks run
- checks skipped
- handoff/repo mismatches found
- residual risk
- final git state, including whether the tag/push happened
