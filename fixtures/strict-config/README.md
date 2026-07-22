# Strict configuration fixtures

This directory defines the accepted and rejected YAML surface for the strict
configuration milestone. [`cases.yaml`](cases.yaml) is the closed,
machine-readable index for every YAML case under `valid/` and `invalid/`.

Each indexed case has one authoritative validation layer:

- `structural`: strict Rust deserialization and the generated Draft 7 JSON
  Schema must agree with `expected`.
- `yaml-parser`: the YAML stream has duplicate keys or multiple documents, so
  it is rejected before there is one JSON instance for schema validation.
- `runtime-only`: the JSON Schema accepts the structural string form, while the
  Rust `glob::Pattern` validator rejects the complete glob grammar.

The two supported rule forms are deliberately exact:

- An include has one nonblank `remote` string and no other properties.
- An executable rule has one nonblank `check` string and may also have `name`,
  `severity`, `fix`, and `hint`.

Legacy `git+...` include locators remain strings accepted by decoding. Their
cache resolution is separate from structural validation. Future `timeout`,
`skip-if`, and `interactive-fix` fields are rejection cases until their complete
runtime slices land.

Optional fields may be omitted, but explicit YAML nulls are rejected. All
configuration strings must be real YAML strings rather than coerced scalars and
must not contain NUL. Empty `cachePath`, `name`, `fix`, and `hint` strings remain
valid for compatibility. Severity input preserves the existing `warning` alias
and ASCII case-insensitive spellings.

Shell text is opaque trusted Bash. Configuration validation checks only that a
`check` is nonblank and that command strings contain no NUL; it does not run a
shell parser. Pattern validation trims the optional leading `!`, rejects an
empty result and NUL, then delegates full syntax to `glob::Pattern`.

The [`integration/`](integration/README.md) tree is intentionally outside the
structural case index. It contains public-CLI assets proving that file, stdin,
nested-include, fix, install, and cached legacy-Git paths all use strict loading
and that invalid input runs no configured command. Scripts under
`valid/scripts/` support the accepted pattern-only and complete configurations.
