# Review Guidelines

Use these guidelines for human and AI review. Follow the
[English collaboration convention](development-flow.md#collaboration-language).

## Review against the Issue

Read the source Issue, its acceptance criteria, the PR description, and the full
diff. Treat the Issue as the reference for the problem and expected outcome.
The central review question is:

> Does the PR completely solve the source Issue, without unnecessary scope or complexity?

## Check for missing work

Verify that every required behavior and acceptance criterion the PR claims to
resolve is implemented and appropriately validated. Do not approve a PR as closing
an Issue when requirements remain unimplemented. Intentionally partial work must
say so and leave the Issue open.

## Check for unnecessary work

Reject additions without a demonstrated need tied to the Issue, including:

- unnecessary abstractions or speculative extensibility;
- unrelated refactoring;
- unneeded frameworks or subsystems;
- extra policies or configuration.

Prefer the smallest coherent design that completely solves the Issue. Added
complexity is not automatically beneficial. Preserve justified existing
conventions, and do not use simplicity to excuse omitted requirements or necessary
validation.

## Check the result and validation

Verify that behavior matches the expected outcome, the change fits the repository,
and affected documentation is accurate. Check that validation is sufficient for
the changed files and uses actual repository tooling. Confirm that claims match
checks actually performed, including failures, skipped checks, and unperformed
checks with reasons.

As described in the [development flow](development-flow.md#2-implement-the-smallest-complete-change),
there are currently no configured build/test commands. For documentation and
templates, inspect Markdown/template syntax, links, anchors, whitespace, and
consistency. Do not require unrelated runtime suites, hardware checks, deployment
verification, or new tooling for a documentation-only change.

## Check pre-merge acceptance and post-merge verification

Require all mandatory pre-merge acceptance criteria to be achievable and verifiable
before merge. Checks possible only after merge must not be pre-merge approval
conditions. This separation does not waive implementation requirements or
applicable pre-merge tests.

Keep optional validation separate from mandatory criteria. Require human
confirmation, physical-device testing, subjective evaluation, or external approval
only when necessary for the actual Issue, with a reason and expected result.

Record genuinely required post-merge verification separately and keep it pending
until performed. For example, verification of a merge-triggered deployment belongs
after merge; pre-merge validation does not prove publication succeeded. Do not add
post-merge checks routinely. None are required for the collaboration templates and
documentation introduced by Issue #1.

## Review outcome and merge

Distinguish these outcomes clearly:

- **Agent-fixable findings:** request changes with the affected location, unmet
  requirement or concrete problem, and expected correction or validation. Review
  the updated PR again after revision.
- **Necessary human input:** record the specific blocking question, why human
  input is necessary, and the expected decision or confirmation. Use the
  workflow's wait path and wait for the answer. Do not repeatedly request code
  fixes for missing human input, or treat elapsed time as confirmation. Reassess
  the PR after the answer arrives.
- **Ready to merge:** approve and merge when the implementation fully addresses
  the Issue it claims to resolve, scope and complexity are justified, validation
  is sufficient, all mandatory pre-merge checks pass, and no blocking findings
  remain.

Pending checks possible only after merge do not block approval or merge when
these conditions are met. Perform any required post-merge verification afterward
and record actual results, including failures and follow-up. Do not report those
checks as passed until they have been performed.
