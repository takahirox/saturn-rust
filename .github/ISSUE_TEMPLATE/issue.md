---
name: Issue
about: Report a problem or propose a change
title: ""
labels: ""
---

<!-- Write the title and body in English. See the development guidance below. -->

## Problem

Describe the problem and why it matters.

## Expected outcome

Describe what should be true when the Issue is resolved.

## Context and reproduction

Add relevant context, examples, logs, or related Issues. For a reproducible problem,
include steps, expected and actual results, and the relevant environment. State
when reproduction steps are not applicable.

## Pre-merge acceptance criteria

List concrete mandatory criteria that can be achieved and verified before merge.
Default to criteria an AI agent can execute and verify. Require human confirmation,
physical-device testing, subjective evaluation, or external approval only when
necessary for this Issue; explain why and what result is expected.

- [ ] Describe a verifiable completion criterion.

Checks possible only after merge must not be prerequisites for pre-merge approval.
See the [development guidance](https://github.com/takahirox/saturn-rust/blob/main/docs/development-flow.md#1-start-with-an-issue).

## Optional validation

List useful additional checks separately from mandatory acceptance criteria, or
state that none are needed.

## Required post-merge verification

Record only necessary checks that can happen only after merge, or state "None."
For example, verification of a merge-triggered deployment belongs here. Report
these checks as pending until performed; pre-merge validation does not prove
publication succeeded. See the [review guidance](https://github.com/takahirox/saturn-rust/blob/main/docs/review-guidelines.md#check-pre-merge-acceptance-and-post-merge-verification).
