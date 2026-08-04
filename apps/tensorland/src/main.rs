use std::process::ExitCode;

fn main() -> ExitCode {
    match tensorland::startup::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(report) = error.diagnostic_report() {
                eprintln!("{report:?}");
            } else {
                eprintln!("tensorland: {error}");
            }
            ExitCode::FAILURE
        }
    }
}
