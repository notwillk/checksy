# Rulesy product family

This repository is the shared workspace for three independently owned products:

| Product | Status | Responsibility |
| --- | --- | --- |
| [Rulesy](products/rulesy/README.md) | Implemented | Provision the current machine from trusted local YAML or stdin. |
| [RulesyOS](products/rulesyos/README.md) | Design only | Boot and recover a verified Linux stage zero that invokes a released Rulesy binary. |
| [Rulesy Compose](products/rulesy-compose/README.md) | Design only | Build, validate, seal, and explicitly publish artifacts by invoking a real pinned Rulesy binary on the host. |

The products share a repository and narrow versioned contracts, not one
lifecycle or release. RulesyOS and Rulesy Compose are not subcommands of
Rulesy. Compose is host tooling and must never be installed in production
RulesyOS firmware.

## Repository layout

```text
products/
  rulesy/             Rust provisioner and its tests, fixtures, and release helpers
  rulesyos/           RulesyOS requirements and implementation design
  rulesy-compose/     Rulesy Compose requirements and implementation design
devcontainer-features/
  src/rulesy/         Published Rulesy development-container Feature
skills/
  rulesy-workflow/    Agent workflow for using Rulesy
docs/
  decisions/          Product-family and repository decisions
```

See the [architecture](ARCHITECTURE.md), [code map](CODEMAP.md), and
[documentation index](docs/README.md) before changing a cross-product
contract. Product-specific details live with the owning product.

## Development

Moon `2.4.5` is the repository task runner. Run tasks from the repository root:

```bash
moon run rulesy:build
moon run rulesy:test
moon run rulesy:format
moon run rulesy:lint
```

Pass task arguments after `--`, for example:

```bash
moon run rulesy:cross-compile -- aarch64-unknown-linux-musl
```

The implemented Rulesy project owns `build`, `format`, `lint`, `test`, and
`release`, together with its development and release helper tasks. The
development-container Feature owns its existing build, shell-lint, and test
commands. RulesyOS, Rulesy Compose, and the workflow skill do not advertise
placeholder lifecycle tasks before they have implementation or established
tooling to run.

Rulesy development details, CLI usage, and its provisioning contract are in
the [Rulesy README](products/rulesy/README.md).
