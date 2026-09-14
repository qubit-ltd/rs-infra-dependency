//! Compatibility entry point for the former `cargo dependency-policy` command.

fn main() {
    qubit_infra_dependency::run_cli(std::env::args_os().collect());
}
