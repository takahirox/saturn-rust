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

Choose checks appropriate to the changed files and the actual repository tooling,
following the [test and CI principles](ci-performance.md#test-and-ci-principles).
Use the actual [build, test and browser smoke commands](setup.md) for application
changes. The [CI performance record](ci-performance.md) documents the complete-suite
inventory, timing commands and publication dependency graph. Do not add
scaffolding, dependencies, CI, or tooling merely to document a process. Inspect the repository again for future changes and use relevant tooling.

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

Use the normal closing reference, such as `Closes #123`, when the implementation
fully addresses the Issue and applicable mandatory pre-merge checks are satisfied.
For intentionally partial work, use a plain reference such as `Related to #123`,
state the remaining scope and unmet acceptance criteria, and leave the parent
Issue open. Pending post-merge-only checks do not make a complete implementation
partial, but their results must not be claimed before they are performed.

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
Required reviews, repository protections, applicable pre-merge tests and genuinely
necessary pre-merge human acceptance remain blockers until satisfied. Checks
possible only after merge are not pre-merge approval conditions.

Preserve `Closes #N` for a fully implemented Issue with satisfied mandatory
pre-merge acceptance, and close the original Issue after successful merge, before
performing required post-merge verification. Do not remove the closing reference,
keep the Issue open, or withhold approval solely because a post-merge-only check
is pending. ProjectWeave publication adds the closing reference for complete work;
review must not undo it for this reason and cause repeated Review → Fix → Publish
→ Review rounds. This does not authorize closing intentionally partial work.

## 6. Perform required post-merge verification, if any

Perform only the necessary checks recorded for the Issue. Keep them pending until
performed, then record actual results, failures, and any required follow-up in the
merged PR, closed Issue, or a linked validation report. Identify the checked
revision and relevant environment. Report unperformed, pending or
environment-blocked checks with reasons; Issue closure does not claim they passed.
Pre-merge validation does not prove deployment or publication succeeded. An
unavailable test environment alone is not evidence of a product defect.

If verification finds an actual defect, first check for an existing equivalent
Issue. Use that Issue or create a new one with reproduction steps, affected
revision, expected and actual behavior, and links to the original Issue and PR.
Record the follow-up link with the verification evidence. Do not reopen the
original Issue solely for post-merge follow-up.

Require post-merge verification only when the actual change needs it. Do not add
routine manual approval, physical-device, heavyweight-model-download or deployment
gates to documentation work. No post-merge verification is required for the
documentation/template changes in Issues #1 and #142.

This lifecycle supersedes older instructions to keep an implemented Issue open
solely for post-merge verification. Preserve historical test outcomes and evidence,
including pending or blocked results; do not rewrite them as passed. Actual missing
implementation or mandatory pre-merge acceptance still blocks completion, including
the partial hardware work tracked by #17 and the unresolved #81 evidence finding.

The normal flow is: Issue → implementation → PR → review → revision as needed →
merge and close the original Issue → required post-merge verification, if any →
new linked defect Issue when needed (reuse an equivalent Issue if one exists).
