# Strict configuration fixtures

This directory defines the accepted and rejected YAML surface for the strict
configuration milestone. [`cases.yaml`](cases.yaml) is the closed,
machine-readable index for every YAML case under `valid/` and `invalid/`.

Each indexed case has one authoritative validation layer:

- `structural`: strict Rust deserialization and the generated Draft 7 JSON
  Schema must agree with `expected`.
- `yaml-parser`: the YAML stream has duplicate keys or multiple documents, so
  it is rejected before there is one JSON instance for schema validation.
- `runtime-only`: the JSON Schema accepts the structural string form, while
  Rust enforces either complete `glob::Pattern` grammar or numeric timeout
  bounds that Draft 7 cannot express over a duration string.

The two supported rule forms are deliberately exact:

- An include has one nonblank `remote` string and no other properties.
- An executable rule has one nonblank `check` string and may also have `name`,
  `severity`, one of `fix` or `interactive-fix`, `hint`, and `timeout`.

Legacy `git+...` include locators remain strings accepted by decoding. Their
cache resolution is separate from structural validation. Includes cannot set a
command timeout. The future `skip-if` field remains a rejection case until its
complete runtime slice lands.

`interactive-fix` is an optional nonblank, NUL-free string on executable rules.
It is mutually exclusive with `fix`; it cannot appear without `check` or on an
include rule. Runtime terminal behavior is covered by the separate interactive
repair corpus.

Executable-rule timeouts use a positive integer followed immediately by `ms`,
`s`, `m`, or `h`. They range from `1ms` through `2h`; omission selects the
compiled 15-minute default. The generated schema enforces the exact string
grammar, including type, null, zero, fractional, unsupported-unit, and NUL
rejection. Numeric overflow and values above `2h` are intentionally
`runtime-only` because Draft 7 cannot compare the numeric portion of this
string. The boundary fixtures are configuration-contract cases; the process
runner corpus separately exercises real timeout behavior without relying on a
1ms scheduling race.

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
