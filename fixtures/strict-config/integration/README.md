# Strict-loading CLI integration fixtures

These files drive the compiled-binary tests in
[`strict_configuration.rs`](../../../src/tests/strict_configuration.rs).
Marker environment variables prove that a valid file or stdin document
executes and that invalid root, nested, repair, and cached-Git documents fail
before any configured command runs.

The `git/` documents are network-free templates. Tests materialize
`remote.yaml` inside a temporary legacy cache at the location described by
`root.yaml`, then replace `git` on `PATH` with a sentinel that must not run.
They repeat that flow with `root-invalid.yaml` and `remote-invalid.yaml` to
prove cached Git configuration uses the same strict decoder.

`install-invalid.yaml` proves an invalid local root is rejected before legacy
Git acquisition. Git transport itself remains a compatibility surface pending
deprecation and is not exercised against a public network.
