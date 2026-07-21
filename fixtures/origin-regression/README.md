# Origin Regression Fixture

This checked-in, network-free fixture proves that the origin-aware CLI executes
each definition's inline rules and pattern scripts from the directory containing
that definition. Run it from the repository root:

```bash
checksy --config=fixtures/origin-regression/.checksy.yaml check
```

The exact stdout is:

```text
✅ root inline origin
✅ nested inline origin
✅ scripts/root-origin.sh
✅ scripts/nested-origin.sh
😎 All rules validated
```

Stderr is empty and the command exits `0`.

## Layout and asset ownership

The root `.checksy.yaml` owns the root `Brewfile`, `template.txt`, and
`scripts/` pattern group. It includes `nested/.checksy.yaml`, which owns a
second `Brewfile`, `template.txt`, and `scripts/` group with the same relative
names but different sentinel contents. Both inline rules and both included Bash
scripts validate their local config and assets, so executing either origin from
the other origin's working directory fails.

The root pattern group runs before the nested group. Within each group,
`scripts/excluded.sh` matches the positive glob and is then removed by that
group's negation. Each excluded script prints a distinct
`*_EXCLUDED_PATTERN_EXECUTED` sentinel and exits nonzero if it ever runs; neither
sentinel may appear in stdout or stderr.

Every executable in this fixture is Bash-only, performs no network access, and
only reads fixture files. A successful run leaves the entire fixture tree
unchanged.
