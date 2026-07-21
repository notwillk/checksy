# Strict configuration fixtures

This corpus defines the accepted and rejected behavior of Checksy's current
legacy YAML configuration loader. The machine-readable [case index](cases.yaml)
is executed by the Rust configuration and generated-schema tests. Every case
names its authoritative validation layer:

- `structural` cases must have the same result through strict Rust
  deserialization and the generated Draft 7 schema.
- `yaml-parser` cases contain duplicate YAML mapping keys. The YAML parser must
  reject them before there is a unique JSON instance for schema validation.
- `runtime-only` cases exercise the complete Rust `glob::Pattern` grammar. The
  schema still checks the value's type, nonempty form, whitespace rules, and NUL
  exclusion, but standard JSON Schema does not reproduce that full grammar.

The index intentionally contains 24 structural cases, two YAML-parser cases,
and one runtime-only case. Tests assert those narrow exception sets so a new
fixture cannot silently bypass structural parity.

Accepted rules have exactly one form:

- A remote reference containing only a non-empty `remote` string.
- An executable rule containing a non-empty `check` string and optional
  `name`, `severity`, `fix`, and `hint` fields.

The rejected fixtures cover unknown and duplicate fields, empty or mixed rule
forms, explicit nulls and incorrectly typed scalars, invalid glob patterns, NUL
bytes in command/path/pattern fields, and fields whose runtime support has not
landed. In particular, `runAs` and per-rule `timeout` remain invalid until their
privilege and process-control milestones are implemented.

Severity input remains case-insensitive for compatibility. Canonical lowercase
spellings are `debug`, `info`, `warn`, `warning`, and `error`; successful CLI
loads warn when a recognized non-lowercase spelling is used, while direct
library deserialization remains quiet. The emitted `checksy schema` document is
generated from the strict Rust model, with only the two layered exceptions
described above.
