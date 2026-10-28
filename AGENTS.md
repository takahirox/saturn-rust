# AI contributor guidance

Follow the [development flow](docs/development-flow.md), including its
[English collaboration convention](docs/development-flow.md#collaboration-language),
and the [review and merge guidelines](docs/review-guidelines.md).

Issue #17 is the active hardware-completion continuation tracker, superseding the
unmet scope of administratively closed Issues #11/#13/#15. Do not reopen them.
Partial changes must use plain references such as "Related to #17", report unmet
acceptance criteria, and leave #17 open. A successful checkpoint, green CI or
intermediate merge is not evidence of hardware completion. Follow #17's autonomous
acceptance policy: no required personal firmware, human testing/approval, live
Pages inspection or investigation/owner confirmation of external Issue closure.
Use original licensed guest fixtures and local/CI native+Wasm+browser checks.
See the [conformance matrix](docs/hardware-conformance.md) for current evidence.

For the partial Issue #81 increment, PR #82 must use `Related to #81` in its
Related Issues section, including during checkpoint publication. Do not regenerate
a closing reference from the Run's source Issue. Keep #81 open and PR #82 in draft
while the mandatory SCU endpoint evidence finding in
[the validation record](docs/validation-issue-81.md#remaining-acceptance-limits)
remains unresolved. This publication rule does not waive that finding.
