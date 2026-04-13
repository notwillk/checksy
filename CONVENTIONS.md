# checksy Coding Conventions

## Naming Conventions

### Files
- **snake_case.rs**: All Rust source files (`check.rs`, `config.rs`)
- **mod.rs style**: Not used; prefer explicit file names

### Structs/Enums
- **PascalCase**: `Config`, `Rule`, `RuleResult`, `CacheManager`
- **Acronyms**: Uppercase when standalone (`GitRemote`), mixed when integrated

### Functions/Variables
- **snake_case**: `resolve_path()`, `expand_remotes()`
- **Boolean queries**: `is_cached()`, `is_remote()` - prefix with `is_`
- **Action verbs**: `load()`, `run()`, `shallow_clone()`

### Constants
- **SCREAMING_SNAKE_CASE**: `DEFAULT_CACHE_PATH`, `GIT_CACHE_DIR`
- **Version**: `VERSION` (simple constant in `version.rs`)

### Type Aliases
- Avoid type aliases; use structs with strong types
- Exception: `HashSet<PathBuf>` is inline, not aliased

## Error Handling Patterns

### Primary Pattern: String Errors
All fallible functions return `Result<T, String>`:
```rust
pub fn load(path: &str) -> Result<Config, String> {
    fs::read_to_string(path).map_err(|e| format!("read config: {}", e))?;
}
```

**Conventions**:
- Prefix error with action: `"read config"`, `"failed to clone"`
- Include underlying error: `format!("...: {}", e)`
- Be specific: `"git remote not cached: {}"` vs generic `"error"`

### Shell Command Errors
```rust
let output = Command::new("bash")...output()?;
if !output.status.success() {
    return Err(format!("command failed: {}", stderr));
}
```

### Error Propagation
- Use `?` for `Result<T, String>` functions
- Use `.ok()` for optional operations (CLI output, logging)
- Prefer early returns over nested matches

### CLI Error Codes
- `0`: Success
- `1`: Help shown (no args) or missing subcommand
- `2`: Config/CLI error (file not found, parse error, missing install)
- `3`: Check failures (rules failed at fail_severity threshold)

## Code Organization

### Module Structure
```rust
// 1. Imports
use crate::schema::{Config, Severity};
use std::fs;

// 2. Constants
const DEFAULT: &str = "value";

// 3. Types (structs, enums)
pub struct Manager { ... }

// 4. impl blocks
impl Manager {
    pub fn new() -> Self { ... }
    fn private_helper() { ... }
}

// 5. Free functions
pub fn utility() -> Result<(), String> { ... }

// 6. Tests (at end)
#[cfg(test)]
mod tests { ... }
```

### Public API Design
- **Explicit exports**: `lib.rs` lists all public items with `pub use`
- **Module visibility**: `pub mod` for modules, selective `pub use` for re-exports
- **Encapsulation**: Prefer `pub(crate)` or private for internal utilities

### Import Style
- **External crates**: Grouped at top
- **Internal modules**: After external, use `crate::` prefix
- **Specific imports**: Import types directly, not whole modules
```rust
use std::fs;
use crate::schema::{Config, Rule};
```

## Testing Approach

### Unit Tests
- **Location**: Bottom of each file in `#[cfg(test)]` module
- **Naming**: `test_<function>_<scenario>()`
- **Style**: Assertive, single assertions where possible
```rust
#[test]
fn test_resolve_path_explicit() {
    let got = resolve_path("/path/to/file");
    assert!(got.is_ok());
    assert_eq!(got.unwrap(), Some("/path/to/file".to_string()));
}
```

### Test Data
- **Fixtures**: YAML configs in `fixtures/` directory
- **Temp files**: Use `tempfile` crate for temporary directories
- **Ignored tests**: Use `#[ignore = "reason"]` for network-dependent tests

### Test Coverage
- **Happy path**: Normal operation
- **Error cases**: Invalid inputs, missing files
- **Edge cases**: Empty inputs, circular references

## Serialization Patterns

### Serde Attributes
```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]  // Config fields
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}
```

**Conventions**:
- `rename_all = "camelCase"` for YAML compatibility
- `default` for optional fields
- `skip_serializing_if = "Option::is_none"` for clean output

### Custom Serialization
Implement `Serialize`/`Deserialize` manually for enums needing string mapping:
```rust
impl Serialize for Severity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let s = match self { Severity::Error => "error", ... };
        serializer.serialize_str(s)
    }
}
```

## Shell Command Execution

### Pattern
```rust
let output = Command::new("bash")
    .current_dir(workdir)
    .arg("-c")
    .arg(&script)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()?;
```

**Conventions**:
- Always use `bash -c` (not direct execution)
- Always set `current_dir()` to config directory
- Capture both stdout and stderr
- Check exit status explicitly

## String Handling

### Formatting
- Use `format!()` for dynamic strings
- Use string literals for static patterns
- Prefer `String` over `&str` for return types (lifetime simplicity)

### CLI Output
- Use `writeln!(stdout/stderr, "...").ok()` (ignore write errors)
- Use emoji for visual status: `✅`, `⚠️`, `❌`, `📦`
- Spinner pattern: Print prefix, print update, print completion

## Comments & Documentation

### Code Comments
- **Why, not what**: Explain reasoning, not obvious behavior
- **Uncertainty**: Mark with `// UNCERTAIN: ...` for future review
- **TODO**: Rare; prefer fixmes only for known issues

### Doc Comments
- **Public APIs**: Use `///` for functions and types
- **Examples**: Include in doc comments for complex functions
```rust
/// Parse a git-based resource locator
/// Format: git+<repo-url>#<ref>:<path>
/// Returns None for non-git remotes
pub fn parse_git_remote(...) -> Option<GitRemote>
```

## Design Patterns

### Builder-Like Construction
Use struct initialization with defaults rather than complex constructors:
```rust
Options {
    config,
    workdir,
    min_severity,
    fail_severity,
}
```

### Error Accumulation
Use early returns and `?` rather than collecting errors:
```rust
fn process() -> Result<T, String> {
    let a = step1()?;  // Early exit on error
    let b = step2()?;
    Ok(combine(a, b))
}
```

### Recursive Traversal
Use `HashSet` for cycle detection:
```rust
fn traverse(item: Item, visited: &mut HashSet<Item>) -> Result<(), String> {
    if !visited.insert(item.clone()) {
        return Ok(());  // Already seen
    }
    // Process children...
}
```

## Anti-Patterns (Avoided)

### Not Used
- **Panics**: Use `Result` instead of `expect()`/`unwrap()` in production code
- **Global state**: All state passed explicitly
- **Macros**: Minimal macro usage (only `format!`, `println!`)
- **Dynamic dispatch**: Rare; prefer concrete types
- **Unsafe**: Not used at all

### Discouraged
- **String concatenation**: Use `format!()` instead of `+`
- **Unnecessary clones**: Pass references where possible
- **Deep nesting**: Flatten with early returns

## File Size Guidelines

- **Target**: ~100-500 lines per file
- **cli.rs exception**: ~950 lines acceptable (main orchestrator)
- **Split trigger**: When distinct responsibility emerges

## Version Management

- **Single source**: `version.rs` contains `VERSION` constant
- **No build.rs**: Version hardcoded, not generated
- **Semantic versioning**: Follow semver for releases
