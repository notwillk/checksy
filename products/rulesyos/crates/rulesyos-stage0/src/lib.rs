mod config;
mod fs_state;
mod hash;
mod process;
mod stage0;
mod status;

use std::ffi::OsString;
use std::io::{self, Write};

pub const VERSION: &str = config::RULESYOS_VERSION;

pub fn run_cli(arguments: impl IntoIterator<Item = OsString>) -> i32 {
    run_cli_with_output(arguments, &mut io::stdout())
}

fn run_cli_with_output(
    arguments: impl IntoIterator<Item = OsString>,
    output: &mut impl Write,
) -> i32 {
    let arguments: Vec<_> = arguments.into_iter().collect();
    match arguments.as_slice() {
        [] => stage0::execute(&config::RuntimeConfig::production(), output),
        [argument] if argument == "--version" => {
            if writeln!(output, "rulesyos-stage0 {VERSION}").is_ok() {
                0
            } else {
                1
            }
        }
        _ => {
            eprintln!("usage: rulesyos-stage0 [--version]");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::run_cli_with_output;
    use std::ffi::OsString;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    pub(crate) fn next_test_id() -> u64 {
        TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn version_is_the_only_accepted_argument() {
        let mut output = Vec::new();
        assert_eq!(
            run_cli_with_output([OsString::from("--version")], &mut output),
            0
        );
        assert_eq!(output, b"rulesyos-stage0 0.1.0\n");

        output.clear();
        assert_eq!(
            run_cli_with_output([OsString::from("--help")], &mut output),
            2
        );
        assert!(output.is_empty());

        assert_eq!(
            run_cli_with_output(
                [OsString::from("--version"), OsString::from("extra")],
                &mut output
            ),
            2
        );
    }
}
