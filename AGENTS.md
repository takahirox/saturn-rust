# AI contributor guidance

Follow the [development flow](docs/development-flow.md), including its
[English collaboration convention](docs/development-flow.md#collaboration-language),
and the [review and merge guidelines](docs/review-guidelines.md).

Issue #15 is the authoritative hardware-completion tracker, superseding the unmet
scope of administratively closed Issues #11/#13. Do not reopen either old issue.
Partial changes must use plain references such as "Related to #15", report unmet
acceptance criteria, and leave #15 open. A successful checkpoint, green CI or
intermediate merge is not evidence of hardware completion. Follow #15's autonomous
acceptance policy: no required personal firmware, human testing/approval, live
Pages inspection or investigation/owner confirmation of external Issue closure.
Use original licensed guest fixtures and local/CI native+Wasm+browser checks.
See the [conformance matrix](docs/hardware-conformance.md) for current evidence.
