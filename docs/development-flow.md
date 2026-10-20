# Development Flow

This is the default contribution flow for saturn-rust, including AI-assisted work.
The default branch is `main`.

## Collaboration language

Use **English** for newly authored templates, development/review/merge
documentation, AI instructions, Issue and PR titles and bodies, review comments,
and commit messages. This collaboration convention does not require translating
historical content or changing product localization.

## 1. Start with an Issue

Use the [Issue template](../.github/ISSUE_TEMPLATE/issue.md) to describe the problem,
expected outcome, relevant context, reproduction steps where applicable, and
concrete completion criteria. An assignee or owner field is not required.

The source Issue defines scope. Read it and relevant existing files before
implementation. Clarify genuinely blocking ambiguity; do not invent requirements.

Default to completion criteria an AI agent can execute and verify. Require human
confirmation, physical-device testing, subjective evaluation, or external approval
only when necessary for the actual Issue. Explain why and what result is expected.
Keep optional validation separate from mandatory completion criteria.

Mandatory pre-merge acceptance must be achievable and verifiable before merge.
Checks possible only after merge must not become pre-merge approval conditions.
Record genuinely necessary post-merge work separately; do not add it routinely.
For example, verifying a merge-triggered deployment belongs after merge, while
applicable checks of the change itself belong before merge.

## 2. Implement the smallest complete change

Choose the smallest coherent design that fully addresses the Issue. Preserve
justified existing conventions and useful content. Avoid unnecessary abstractions,
speculative extensibility, unrelated refactoring, frameworks or subsystems, and
extra policies or configuration without a demonstrated need tied to the Issue.
Added complexity is not automatically beneficial; simplicity does not excuse
missing requirements or necessary validation.

Choose checks appropriate to the changed files and the actual repository tooling.
The repository currently has no application files or configured build, test, lint,
or runtime commands. Do not invent commands or add application scaffolding,
dependencies, CI, or tooling merely to document a process. Inspect the repository
again for future changes and use any relevant tooling that actually exists then.

For templates and documentation, check Markdown and template syntax, relative
links and anchors, whitespace, and consistency across the files. Use available
tooling and manual inspection as appropriate. Git's `git diff --check` checks
unstaged tracked changes for whitespace errors; use `git diff --cached --check`
for staged changes, including new files. Documentation-only changes do not require
unrelated runtime tests, physical-device checks, or deployed-site verification.

## 3. Create a Pull Request

Target `main` unless the Issue identifies another integration branch. Use the
[PR template](../.github/pull_request_template.md) to explain:

- the problem, resulting behavior, and focused changes;
- the related source Issue and whether the implementation fully resolves it;
- validation actually performed and its results, including failures, skipped
  checks, and unperformed checks with reasons;
- any genuinely required post-merge verification, reported separately as pending,
  or "None."

Only claim to close an Issue when the implementation fully addresses it. State
intentionally partial work clearly and leave the Issue open. Necessary pending
post-merge checks do not make a complete implementation partial, but their results
must not be claimed before they are performed.

## 4. Review and revise as needed

Every PR should be reviewed before merge using the
[review guidelines](review-guidelines.md). The central question is:

> Does the PR completely solve the source Issue, without unnecessary scope or complexity?

Check both missing requirements and unjustified additions. Request fixes for
agent-fixable findings and review the updated PR again. If genuinely necessary
human input is missing, record the specific question and expected answer, then
use the workflow's wait path until the answer arrives. Do not repeatedly request
code fixes for a decision or confirmation that requires human input.

## 5. Merge

Approve and merge when the implementation is complete, validation is sufficient,
all mandatory pre-merge checks pass, and no blocking review findings remain.
Checks possible only after merge are not pre-merge approval conditions.

## 6. Perform required post-merge verification, if any

Perform only the necessary checks recorded for the Issue. Keep them pending until
performed, then record actual results, failures, and any required follow-up.
Pre-merge validation does not prove deployment or publication succeeded.
No post-merge verification is required for the collaboration templates and
documentation introduced by Issue #1.

The normal flow is: Issue → implementation → PR → review → revision as needed →
merge → required post-merge verification, if any.
