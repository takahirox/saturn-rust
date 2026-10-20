# saturn-rust

An original Rust SH-2 interpreter with a minimal WebAssembly browser client.
This first executable slice runs one master CPU and a small homebrew fixture.
It does not boot commercial games or implement Saturn BIOS services yet.

- [Build, run and test](docs/setup.md)
- [Architecture, supported instructions, memory map and roadmap](docs/architecture.md)
- [Issue #3 validation results](docs/validation-issue-3.md)

Choose **HLE bootstrap prototype** to run without a BIOS: Reset, then Run 6
instructions yields `R0=4`, `R1=7`, `PC=0x0600000E`. Alternatively, choose
**User-provided BIOS** and select a legally obtained raw 512 KiB file. It stays
local to your browser, is mapped read-only, and supplies the CPU reset vectors.
Unsupported instructions stop execution with a diagnostic; a BIOS splash screen,
sound and game compatibility are outside this milestone.

The public browser prototype is published at
**https://takahirox.github.io/saturn-rust/** once Pages is enabled and the first
successful deployment finishes. It contains no Sega BIOS or game image; selected
BIOS bytes stay in the browser with no upload, storage or telemetry.

The **Browser Pages** workflow builds and tests each push/merge to `main`, then
deploys the generated `web/` site. Pull requests targeting `main` run the same
validation without publishing. Generated `web/pkg/` files are never committed.
A repository administrator may first need to select **Settings → Pages → Build
and deployment → Source: GitHub Actions** and allow Actions/deployments. The
workflow does not change that repository setting.

Inspect build, browser test and deployment logs under **Actions → Browser Pages**;
the deployment status/link also appears in the `github-pages` environment. Fix
failures with a new `main` commit, or rerun the latest `main` workflow after a
transient failure or Pages setup. Superseded revisions are skipped, including old
reruns. See [Pages setup and post-merge verification](docs/setup.md#github-pages)
for the publication checks that remain necessary after merge.

Contributor and AI agent guidance:

- [Development flow and English collaboration convention](docs/development-flow.md)
- [Review and merge guidelines](docs/review-guidelines.md)
- [Issue template](.github/ISSUE_TEMPLATE/issue.md)
- [Pull request template](.github/pull_request_template.md)
