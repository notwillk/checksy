# Rulesy monorepo code map

```text
.
├── .devcontainer/                 Development environment and Rulesy dogfooding
├── .moon/
│   └── workspace.yml              Explicit five-project Moon map
├── .github/workflows/             CI and Rulesy release automation
├── devcontainer-features/
│   └── src/rulesy/                Published Rulesy Feature
├── docs/
│   ├── README.md                  Decision and design index
│   └── decisions/
│       ├── rulesy-product-family.md  Historical rename/family decision
│       └── monorepo.md            Current repository-boundary decision
├── products/
│   ├── rulesy/
│   │   ├── Cargo.toml             Independent Rulesy Rust package/workspace
│   │   ├── src/                   CLI and library implementation
│   │   ├── tests/                 Compiled-binary contract tests
│   │   ├── fixtures/              Closed, network-free fixture corpora
│   │   ├── scripts/               Build/release implementation helpers
│   │   ├── docs/                  Rulesy-specific operational docs
│   │   ├── README.md              User and provisioning contract
│   │   └── ARCHITECTURE.md        Detailed Rulesy runtime architecture
│   ├── rulesyos/
│   │   ├── Cargo.toml             Independent RulesyOS Rust workspace
│   │   ├── crates/rulesyos-stage0/  Permanent boot-time orchestrator
│   │   ├── br2-external/          Buildroot board, packages, and defconfig
│   │   ├── scripts/               Pinned Buildroot preparation and build
│   │   ├── tests/runtime/         KVM image and known-good tests
│   │   ├── README.md              Product status and command index
│   │   └── docs/design.md         Stage-zero and production OS design
│   └── rulesy-compose/
│       ├── README.md              Product status and design index
│       └── docs/design.md         Host composition design
├── scripts/
│   ├── install.sh                 Stable public Rulesy installer path
│   └── uninstall.sh               Stable public Rulesy uninstaller path
├── skills/
│   └── rulesy-workflow/           Rulesy agent workflow package
├── ARCHITECTURE.md                Family boundaries and contracts
├── CONVENTIONS.md                 Monorepo contribution rules
├── PROMPT.md                      Family-level agent context
└── todo.md                        Product-family roadmap and repository work
```

## Ownership map

| Area | Owner | Notes |
| --- | --- | --- |
| `products/rulesy/` | Rulesy | Implemented product; detailed code map is in its architecture and prompt. |
| `products/rulesyos/` | RulesyOS | Functional baked-configuration reference image; production boot, external bundles, updates, and releases remain deferred. |
| `products/rulesy-compose/` | Rulesy Compose | Documentation-only until an approved implementation milestone. |
| `devcontainer-features/src/rulesy/` | Rulesy release integration | Preserves the published `rulesy` Feature identity. |
| `.devcontainer/` | Repository development environment | Bootstraps Rulesy, then converges developer tools. |
| `docs/decisions/` | Product family | Accepted and historical cross-product decisions. |
| `skills/rulesy-workflow/` | Rulesy integration | Uses the public Rulesy contract; it is not product runtime. |

## Important dependency rules

- Rulesy has no dependency on RulesyOS or Rulesy Compose.
- RulesyOS consumes a released, pinned Rulesy artifact.
- Rulesy Compose invokes a real pinned Rulesy and may consume a released
  RulesyOS base.
- Compose binaries and publisher credentials never enter firmware.
- Product Cargo workspaces and release lifecycles remain independent.

## Task orchestration

Moon `2.4.5` is the repository task runner. `.moon/workspace.yml` maps the five
repository projects, while each project's `moon.yml` owns its applicable
tasks. Rulesy's task definitions delegate cross-compilation and release to its
product-owned scripts. Run tasks as `moon run <project>:<task>` and pass
runtime arguments after `--`.
