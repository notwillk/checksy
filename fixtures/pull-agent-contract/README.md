# Pull-Agent Contract Fixtures

These files describe accepted and rejected behavior for the planned pull-based
agent. They are a contract for future tests; current production code does not
parse this corpus or implement the outcomes.

[cases.yaml](cases.yaml) is the machine-readable index. Each entry has a stable
ID, relative path, accept/reject result, first wiring milestone, and reason.
Behavioral assertions live in [scenarios.yaml](scenarios.yaml).

Expected accept/reject describes whether the requested transition, fallback, or
promotion is permitted; it does not merely describe whether YAML parses.

## Layout

- policy contains privilege-policy examples and abstract prohibited enrollment
  exception cases. They are fragments, not the deferred enrollment schema.
- local covers defaults, ambiguity, confinement, and protected external roots.
- authentication/https contains a real Minisign verification vector.
- authentication/git contains accepted and rejected allowed-signers examples.
- scenarios.yaml covers duration, apply, status, offline, rollback, Git
  freshness, and unsigned behavior.

## Crypto fixtures

The HTTPS manifest is signed over its exact LF-terminated bytes. The private test
key was discarded. Verify it with:

~~~bash
cd fixtures/pull-agent-contract
minisign -Vm authentication/https/manifest.json \
  -p authentication/https/minisign.pub \
  -x authentication/https/manifest.json.minisig
~~~

The tampered JSON remains valid JSON but must fail against the same sidecar:

~~~bash
cd fixtures/pull-agent-contract
! minisign -Vm authentication/https/manifest.tampered.json \
  -p authentication/https/minisign.pub \
  -x authentication/https/manifest.json.minisig
~~~

The fixture-local .gitattributes forces LF endings so autocrlf cannot alter the
signed bytes. The SSH fixtures contain only a test public key; no private
Minisign or SSH key is committed.

## Future harness requirements

- Materialize allowed-external-roots.yaml.in by replacing __EXTERNAL_ROOT__ with
  the canonical absolute path of local/external-assets.
- Create symlink variants at runtime rather than committing symlinks.
- Treat shell checks as arbitrary code. Path cases exercise recognized
  pattern/config resolution and do not claim shell sandboxing.
- Expand the http-5xx scenario token to every integer status from 500 through
  599.
- Apply the general duration acceptance list to syntax only; numeric limits from
  P0-3 may reject otherwise well-formed values such as 999h.
- Verify every path in cases.yaml before executing a case.
