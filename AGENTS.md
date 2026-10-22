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
