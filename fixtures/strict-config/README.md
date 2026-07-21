# Strict configuration fixtures

This corpus defines the accepted and rejected behavior of Checksy's current
legacy YAML configuration loader. `cases.yaml` is executed by the Rust
configuration tests.

Accepted rules have exactly one form:

- A remote reference containing only a non-empty `remote` string.
- An executable rule containing a non-empty `check` string and optional
  `name`, `severity`, `fix`, and `hint` fields.

The rejected fixtures cover unknown and duplicate fields, empty or mixed rule
forms, explicit nulls and incorrectly typed scalars, invalid glob patterns, and
fields whose runtime support has not landed. In particular, `runAs` and
per-rule `timeout` remain invalid until their privilege and process-control
milestones are implemented.

The emitted `checksy schema` document is still hand-maintained. Runtime/schema
parity and generated schema work belong to the next P1 roadmap item.
