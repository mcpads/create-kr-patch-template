use std::process::ExitCode;

fn main() -> ExitCode {
    match patch_guard_mcp::serve_stdio() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("patch-guard-mcp: {error}");
            ExitCode::FAILURE
        }
    }
}
