# Remote Config Test Fixtures

This directory contains test fixtures for the remote config feature.

## Basic Tests

### Basic Remote (`./.checksy.yaml` + `shared.yaml`)
Simple case showing a main config that includes rules from a shared config file.

```bash
cd fixtures/remote-config
checksy check
```

Expected: 4 rules total (2 local + 2 from shared, one of which fails)

### Defaults Inheritance (`inherit-parent.yaml` + `no-severity.yaml`)
Tests that remote configs inherit `checkSeverity` from parent configs.

```bash
cd fixtures/remote-config
checksy check --config=inherit-parent.yaml
```

Expected: Rules from no-severity.yaml should have `warn` severity inherited.

### Preconditions (`with-preconditions.yaml` + `shared-preconditions.yaml`)
Tests that remote configs work in preconditions section.

```bash
cd fixtures/remote-config
checksy check --config=with-preconditions.yaml
```

## Complex Scenarios

### Nested Remotes (`nested/`)
Linear chain: top → middle → bottom (not circular)

```bash
cd fixtures/remote-config/nested
checksy check --config=top.yaml
```

Expected: 3 rules (one from each config)

### Circular References (`circular/`)
Circular chain: a → b → c → a

```bash
cd fixtures/remote-config/circular
checksy check --config=a.yaml
```

Expected: 3 rules (A, B, C - A's remote to B is skipped since A was already visited when C tries to reference it)

## Invalid Cases (`invalid/`)

These configs should fail to load with clear error messages:

### Remote with Check Property (`with-check.yaml`)
Should fail: remote rule has `check` property (only `remote` allowed)

```bash
cd fixtures/remote-config/invalid
checksy check --config=with-check.yaml
# Error: invalid remote rule (remote: ...): remote rule cannot have properties: check, name
```

### Remote with Severity (`with-severity.yaml`)
Should fail: remote rule has `severity` property

```bash
cd fixtures/remote-config/invalid
checksy check --config=with-severity.yaml
# Error: invalid remote rule (remote: ...): remote rule cannot have properties: severity
```

### Missing File (`missing-file.yaml`)
Should fail: referenced file doesn't exist

```bash
cd fixtures/remote-config/invalid
checksy check --config=missing-file.yaml
# Error: remote config 'nonexistent-file.yaml' not found
```
