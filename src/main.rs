use std::io;

fn run(args: Vec<String>, stdout: &mut dyn io::Write, stderr: &mut dyn io::Write) -> i32 {
    checksy::run(args, stdout, stderr)
}

fn main() {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    std::process::exit(run(std::env::args().skip(1).collect(), &mut stdout, &mut stderr));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_help_command() {
        let mut stdout = vec![];
        let mut stderr = vec![];
        let code = run(vec!["help".to_string()], &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(!stdout.is_empty());
    }
}