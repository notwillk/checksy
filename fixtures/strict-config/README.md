# Strict configuration fixtures

This corpus defines the accepted and rejected behavior of Checksy's current
legacy YAML configuration loader. The machine-readable [case index](cases.yaml)
is executed by the Rust configuration and generated-schema tests. Every case
names its authoritative validation layer:

- `structural` cases must have the same result through strict Rust
  deserialization and the generated Draft 7 schema.
- `yaml-parser` cases contain duplicate YAML mapping keys. The YAML parser must
  reject them before there is a unique JSON instance for schema validation.
- `runtime-only` cases exercise the complete Rust `glob::Pattern` grammar and
  the numeric range and hard maximum for duration strings. The schema still
  checks glob type, nonempty form, whitespace rules, and NUL exclusion, and it
  enforces duration syntax, but standard JSON Schema cannot express numeric
  limits over the digits inside a string.

The index intentionally contains 27 structural cases, two YAML-parser cases,
and three runtime-only cases. Tests assert those narrow exception sets so a new
fixture cannot silently bypass structural parity.

Accepted rules have exactly one form:

- A remote reference containing only a non-empty `remote` string.
- An executable rule containing a non-empty `check` string and optional
  `name`, `severity`, `timeout`, `fix`, and `hint` fields. A timeout uses the
  strict `^[1-9][0-9]*(ms|s|m|h|d)$` fixed-unit syntax and cannot exceed `2h`.

The rejected fixtures cover unknown and duplicate fields, empty or mixed rule
forms, explicit nulls and incorrectly typed scalars, invalid glob patterns, NUL
bytes in command/path/pattern fields, invalid or excessive timeouts, and fields
whose runtime support has not landed. `runAs` remains invalid until its
privilege milestone is implemented. A remote rule cannot specify a timeout.

Severity input remains case-insensitive for compatibility. Canonical lowercase
spellings are `debug`, `info`, `warn`, `warning`, and `error`; successful CLI
loads warn when a recognized non-lowercase spelling is used, while direct
library deserialization remains quiet. The emitted `checksy schema` document is
generated from the strict Rust model, with only the layered exceptions
described above.
