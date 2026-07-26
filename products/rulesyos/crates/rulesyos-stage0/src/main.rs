fn main() {
    std::process::exit(rulesyos_stage0::run_cli(std::env::args_os().skip(1)));
}
