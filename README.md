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

Contributor and AI agent guidance:

- [Development flow and English collaboration convention](docs/development-flow.md)
- [Review and merge guidelines](docs/review-guidelines.md)
- [Issue template](.github/ISSUE_TEMPLATE/issue.md)
- [Pull request template](.github/pull_request_template.md)
