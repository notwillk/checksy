# Interactive-fix contract fixtures

This closed, network-free corpus exercises terminal-capable repairs through the
compiled `checksy` binary. [`cases.yaml`](cases.yaml) maps every checked-in YAML
configuration and shell asset to an executable Rust integration test. Tests copy
the corpus to a temporary directory before running cases that create markers, so
this directory remains unchanged.

An `interactive-fix` is considered only during `check --fix` after its check
fails. Eligible file-backed runs use a real controlling terminal and an inner
PTY; Checksy adds no confirmation prompt. The configured command owns its
interaction. A successful repair receives one non-interactive final check. A
completed nonzero repair keeps the original rule failure, skips that final
check, and allows later rules to run.

Terminal use is prohibited by `--non-interactive` and by both stdin
configuration spellings. Those cases leave a needed repair unexecuted, report a
reason-specific compliance failure, and continue. Stdin takes diagnostic
precedence when `--non-interactive` is also present. Passing rules never probe
for a terminal, and ordinary `fix` remains available in every headless mode.

The PTY cases cover input through both the repair's standard input and
`/dev/tty`, live merged terminal output, window/terminal restoration, timeout
escalation, close-on-exec terminal descriptors, unsupported job-control
suspension, outer job-control protection, foreground revocation without
post-loss relay, managed descendants, and parent interruption. The isolated
helpers are ignored tests in the Rust integration-test executable. Readiness
uses bounded pipe, socket, and
advisory-lock handshakes rather than sleeps. All commands are local Bash and
require no public network.

Run the executable contract from the crate directory:

```text
cargo test --locked --test interactive_fix_contract
```

These fixtures prove terminal plumbing and supervision; they do not sandbox the
trusted repair command. A repair retains the invoking user's filesystem,
network, and process authority, and a command that deliberately creates another
session can escape the managed process group.
