<!-- Write the title and body in English. See the development flow linked below. -->

## Summary

Explain the problem and the focused changes that address it.

## Outcome

Describe the resulting behavior, including before/after behavior when useful.
Explain any intentionally partial scope, remaining work and unmet acceptance
criteria. Pending post-merge-only verification does not make a complete
implementation partial.

## Validation

List the checks actually performed and their results. Report failures, skipped
checks, and unperformed checks with reasons. Choose checks appropriate to the
changed files and existing repository tooling; do not invent commands or require
unrelated runtime tests for a documentation-only change.

## Required post-merge verification

List only necessary checks possible after merge and mark them pending until
performed, or state "None." Keep these separate from pre-merge validation.
After merge and Issue closure, record results or pending/blocked status in this
PR, the closed Issue or a linked validation report. Follow the
[post-merge lifecycle](https://github.com/takahirox/saturn-rust/blob/main/docs/development-flow.md#6-perform-required-post-merge-verification-if-any)
for discovered defects and new linked Issues.

## Related Issues

Link the source Issue and any other relevant Issues. Use and preserve `Closes #123`
when the implementation fully resolves that Issue and mandatory pre-merge checks
are satisfied, including when post-merge-only verification is pending. For
intentionally partial work, use `Related to #123` and leave the parent Issue open.

See the [development flow](https://github.com/takahirox/saturn-rust/blob/main/docs/development-flow.md) and
[review guidelines](https://github.com/takahirox/saturn-rust/blob/main/docs/review-guidelines.md).
