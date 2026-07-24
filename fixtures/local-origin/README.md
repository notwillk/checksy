# Local-origin contract

This closed, network-free corpus proves that a local include graph retains the
defining directory of every executable rule and pattern group. [`cases.yaml`](cases.yaml)
maps the checked-in workflow to
[`local_origin_contract.rs`](../../src/tests/local_origin_contract.rs).

The root and child definitions deliberately own same-named `Brewfile` and
`template.txt` assets with different contents. The child predicate, initial
check, ordinary repair, and final check all validate the child's copies. Root
patterns run before child patterns, and each group has its own negations.
Excluded scripts write a forbidden sentinel and fail if they are ever invoked.

The executable test selects `root.yaml` by absolute path while Checksy's current
directory is an unrelated temporary directory. Trace and repair state also live
outside this fixture, and the test snapshots the complete corpus before and
after execution to prove that the checked-in files remain unchanged.

Expected trace:

```text
root-check
child-skip
child-check
child-fix
child-check
root-pattern
child-pattern
```

Run the contract from the crate directory:

```text
cargo test --locked --test local_origin_contract
```
