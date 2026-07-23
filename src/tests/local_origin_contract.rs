use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod support;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalOriginCorpus {
    schema_version: u64,
    cases: Vec<LocalOriginCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalOriginCase {
    id: String,
    fixture: String,
    assets: Vec<String>,
    mode: String,
    test: String,
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("local-origin")
}

fn corpus() -> LocalOriginCorpus {
    let document = fs::read_to_string(fixture_root().join("cases.yaml")).unwrap();
    let corpus: LocalOriginCorpus = serde_yaml::from_str(&document).unwrap();
    assert_eq!(corpus.schema_version, 1);
    corpus
}

fn collect_files(directory: &Path, root: &Path, files: &mut BTreeSet<String>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_files(&path, root, files);
        } else {
            files.insert(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn fixture_snapshot() -> BTreeMap<String, Vec<u8>> {
    let root = fixture_root();
    let mut files = BTreeSet::new();
    collect_files(&root, &root, &mut files);
    files
        .into_iter()
        .map(|path| {
            let contents = fs::read(root.join(&path)).unwrap();
            (path, contents)
        })
        .collect()
}

fn checksy() -> Command {
    Command::new(env!("CARGO_BIN_EXE_checksy"))
}

fn capture(mut command: Command, stdin: Option<&[u8]>) -> Output {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(input) = stdin {
        command.stdin(Stdio::piped());
        let mut child = command.spawn().unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    } else {
        command.stdin(Stdio::null()).output().unwrap()
    }
}

fn output_context(output: &Output) -> String {
    format!(
        "exit={:?} stdout={:?} stderr={:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{}",
        output_context(output)
    );
}

#[test]
fn corpus_is_closed_unique_network_free_and_mapped_to_test() {
    let corpus = corpus();
    assert_eq!(corpus.cases.len(), 1);
    let case = &corpus.cases[0];
    assert_eq!(case.id, "absolute-root-with-child");
    assert_eq!(case.fixture, "root.yaml");
    assert_eq!(case.mode, "absolute-file-from-unrelated-cwd");
    assert_eq!(
        case.test,
        "absolute_local_includes_preserve_origins_end_to_end"
    );

    let indexed: BTreeSet<_> = std::iter::once(&case.fixture)
        .chain(case.assets.iter())
        .cloned()
        .collect();
    assert_eq!(
        indexed.len(),
        case.assets.len() + 1,
        "fixture index repeats a path"
    );

    let root = fixture_root();
    let mut actual = BTreeSet::new();
    collect_files(&root, &root, &mut actual);
    actual.remove("README.md");
    actual.remove("cases.yaml");
    assert_eq!(indexed, actual, "fixture index is not closed");

    for path in fixture_snapshot().keys() {
        let bytes = fs::read(root.join(path)).unwrap();
        assert!(!bytes.contains(&b'\r'), "{path} must use LF endings");
        assert_eq!(bytes.last(), Some(&b'\n'), "{path} needs a final newline");
        let text = String::from_utf8(bytes).unwrap().to_ascii_lowercase();
        for forbidden in ["http://", "https://", "ssh://", "git@", "git+"] {
            assert!(
                !text.contains(forbidden),
                "{path} contains forbidden network token {forbidden:?}"
            );
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn absolute_local_includes_preserve_origins_end_to_end() {
    let _serial = support::provisioning_test_guard();
    let before = fixture_snapshot();
    let state = tempfile::tempdir().unwrap();
    let unrelated = tempfile::tempdir().unwrap();
    let root = fixture_root().canonicalize().unwrap();
    let config = root.join("root.yaml");
    let trace = state.path().join("trace");
    let fixed = state.path().join("fixed");
    let forbidden = state.path().join("forbidden");

    let output = capture(
        {
            let mut command = checksy();
            command
                .current_dir(unrelated.path())
                .arg("--config")
                .arg(&config)
                .arg("check")
                .arg("--fix")
                .arg("--non-interactive")
                .env("CHECKSY_LOCAL_ORIGIN_TRACE", &trace)
                .env("CHECKSY_LOCAL_ORIGIN_FIXED", &fixed)
                .env("CHECKSY_LOCAL_ORIGIN_FORBIDDEN", &forbidden);
            command
        },
        None,
    );

    assert_exit(&output, 0);
    assert!(output.stderr.is_empty(), "{}", output_context(&output));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "✅ root inline origin\n",
            "⚠️  child relative repair\n",
            "✅ child relative repair fix\n",
            "✅ child relative repair\n",
            "✅ patterns/root.sh\n",
            "✅ patterns/child.sh\n",
            "😎 All rules validated\n",
        )
    );
    assert_eq!(
        fs::read_to_string(&trace).unwrap(),
        concat!(
            "root-check\n",
            "child-skip\n",
            "child-check\n",
            "child-fix\n",
            "child-check\n",
            "root-pattern\n",
            "child-pattern\n",
        )
    );
    assert!(fixed.is_file(), "child fix did not materialize state");
    assert!(
        !forbidden.exists(),
        "an origin-scoped pattern negation did not exclude its failing script"
    );
    assert_eq!(
        fixture_snapshot(),
        before,
        "the checked-in fixture changed during provisioning"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn active_cycle_is_a_preflight_error_with_an_ordered_chain() {
    let directory = tempfile::tempdir().unwrap();
    let unrelated = tempfile::tempdir().unwrap();
    let marker = directory.path().join("must-not-run");
    let a = directory.path().join("a.yaml");
    let b = directory.path().join("b.yaml");
    let c = directory.path().join("c.yaml");

    fs::write(
        &a,
        concat!(
            "rules:\n",
            "  - name: cycle marker must not run\n",
            "    check: ': > \"$CHECKSY_LOCAL_ORIGIN_MARKER\"'\n",
            "  - remote: b.yaml\n",
        ),
    )
    .unwrap();
    fs::write(&b, "rules:\n  - remote: c.yaml\n").unwrap();
    fs::write(&c, "rules:\n  - remote: a.yaml\n").unwrap();

    let output = capture(
        {
            let mut command = checksy();
            command
                .current_dir(unrelated.path())
                .arg("--config")
                .arg(&a)
                .arg("check")
                .env("CHECKSY_LOCAL_ORIGIN_MARKER", &marker);
            command
        },
        None,
    );

    assert_exit(&output, 2);
    assert!(output.stdout.is_empty(), "{}", output_context(&output));
    assert!(
        !marker.exists(),
        "cycle preflight executed a configured rule"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("local include cycle detected"),
        "{}",
        output_context(&output)
    );
    let expected_chain = "a.yaml -> b.yaml -> c.yaml -> a.yaml";
    assert!(
        stderr.contains(expected_chain),
        "cycle diagnostic omitted ordered chain {expected_chain:?}: {stderr:?}"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn completed_diamond_include_executes_once_at_first_seen_position() {
    let directory = tempfile::tempdir().unwrap();
    let unrelated = tempfile::tempdir().unwrap();
    let trace = directory.path().join("trace");
    let root = directory.path().join("root.yaml");

    fs::write(
        &root,
        "rules:\n  - remote: left.yaml\n  - remote: right.yaml\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("left.yaml"),
        concat!(
            "rules:\n",
            "  - name: diamond left\n",
            "    check: \"printf 'left\\\\n' >> \\\"$CHECKSY_LOCAL_ORIGIN_TRACE\\\"\"\n",
            "  - remote: shared.yaml\n",
        ),
    )
    .unwrap();
    fs::write(
        directory.path().join("right.yaml"),
        concat!(
            "rules:\n",
            "  - name: diamond right\n",
            "    check: \"printf 'right\\\\n' >> \\\"$CHECKSY_LOCAL_ORIGIN_TRACE\\\"\"\n",
            "  - remote: shared.yaml\n",
        ),
    )
    .unwrap();
    fs::write(
        directory.path().join("shared.yaml"),
        concat!(
            "rules:\n",
            "  - name: diamond shared\n",
            "    check: \"printf 'shared\\\\n' >> \\\"$CHECKSY_LOCAL_ORIGIN_TRACE\\\"\"\n",
        ),
    )
    .unwrap();

    let output = capture(
        {
            let mut command = checksy();
            command
                .current_dir(unrelated.path())
                .arg("--config")
                .arg(&root)
                .arg("check")
                .env("CHECKSY_LOCAL_ORIGIN_TRACE", &trace);
            command
        },
        None,
    );

    assert_exit(&output, 0);
    assert!(output.stderr.is_empty(), "{}", output_context(&output));
    assert_eq!(fs::read_to_string(trace).unwrap(), "left\nshared\nright\n");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "✅ diamond left\n",
            "✅ diamond shared\n",
            "✅ diamond right\n",
            "😎 All rules validated\n",
        )
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn stdin_remains_cwd_rooted_and_self_contained() {
    let directory = tempfile::tempdir().unwrap();
    let trace = directory.path().join("trace");
    let marker = directory.path().join("include-must-not-run");
    fs::write(directory.path().join("asset.txt"), "stdin-origin\n").unwrap();
    fs::create_dir(directory.path().join("patterns")).unwrap();
    fs::write(
        directory.path().join("patterns/stdin.sh"),
        concat!(
            "#!/usr/bin/env bash\n",
            "set -euo pipefail\n",
            "[[ $(<asset.txt) == stdin-origin ]]\n",
            "printf 'stdin-pattern\\n' >> \"$CHECKSY_LOCAL_ORIGIN_TRACE\"\n",
        ),
    )
    .unwrap();

    let document = concat!(
        "rules:\n",
        "  - name: stdin cwd origin\n",
        "    skip-if: |\n",
        "      set -eu\n",
        "      test \"$(cat ./asset.txt)\" = \"stdin-origin\"\n",
        "      printf 'stdin-skip\\n' >> \"$CHECKSY_LOCAL_ORIGIN_TRACE\"\n",
        "      exit 1\n",
        "    check: |\n",
        "      set -eu\n",
        "      test \"$(cat ./asset.txt)\" = \"stdin-origin\"\n",
        "      printf 'stdin-check\\n' >> \"$CHECKSY_LOCAL_ORIGIN_TRACE\"\n",
        "patterns:\n",
        "  - patterns/*.sh\n",
    );
    let output = capture(
        {
            let mut command = checksy();
            command
                .current_dir(directory.path())
                .arg("--stdin-config")
                .arg("check")
                .env("CHECKSY_LOCAL_ORIGIN_TRACE", &trace);
            command
        },
        Some(document.as_bytes()),
    );
    assert_exit(&output, 0);
    assert!(output.stderr.is_empty(), "{}", output_context(&output));
    assert_eq!(
        fs::read_to_string(&trace).unwrap(),
        "stdin-skip\nstdin-check\nstdin-pattern\n"
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "✅ stdin cwd origin\n",
            "✅ patterns/stdin.sh\n",
            "😎 All rules validated\n",
        )
    );

    fs::write(
        directory.path().join("child.yaml"),
        concat!(
            "rules:\n",
            "  - name: stdin include must not run\n",
            "    check: ': > \"$CHECKSY_LOCAL_ORIGIN_MARKER\"'\n",
        ),
    )
    .unwrap();
    let rejected = capture(
        {
            let mut command = checksy();
            command
                .current_dir(directory.path())
                .arg("--config")
                .arg("-")
                .arg("check")
                .env("CHECKSY_LOCAL_ORIGIN_MARKER", &marker);
            command
        },
        Some(b"rules:\n  - remote: child.yaml\n"),
    );
    assert_exit(&rejected, 2);
    assert!(rejected.stdout.is_empty(), "{}", output_context(&rejected));
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("stdin configuration must be self-contained"),
        "{}",
        output_context(&rejected)
    );
    assert!(!marker.exists(), "stdin include executed a configured rule");
}
