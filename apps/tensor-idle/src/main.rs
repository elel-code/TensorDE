use std::{env, io, process::ExitCode};

use tensor_idle::{IdleConfig, IdleMonitorRuntime, IdlePlan, PowerSource};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = IdleConfig::load_default_path()?;
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let check = arguments.iter().any(|argument| argument == "--check");
    let check_wayland = arguments
        .iter()
        .any(|argument| argument == "--check-wayland");
    let observe = arguments.iter().any(|argument| argument == "--observe");
    let battery = arguments.iter().any(|argument| argument == "--battery");
    let source = if battery {
        PowerSource::Battery
    } else {
        PowerSource::Ac
    };
    let plan = IdlePlan::compile(&config, source);
    if check && !check_wayland && !observe {
        println!(
            "tensor-idle: source={source:?}, enabled={}, inhibitors={}, stages={}",
            plan.enabled,
            plan.respect_inhibitors,
            plan.stages.len()
        );
        return Ok(());
    }
    if check_wayland {
        let runtime = IdleMonitorRuntime::connect(&plan)?;
        println!(
            "tensor-idle: registered {} {source:?} Wayland idle monitors",
            runtime.monitor_count()
        );
        return Ok(());
    }
    if observe {
        let mut runtime = IdleMonitorRuntime::connect(&plan)?;
        let mut transitions = Vec::with_capacity(plan.stages.len());
        loop {
            transitions.clear();
            runtime.dispatch_into(None, &mut transitions)?;
            for transition in &transitions {
                println!(
                    "tensor-idle: action={:?} idle={} after_ms={}",
                    transition.action, transition.idle, transition.after_ms
                );
            }
        }
    }
    Err(io::Error::other(
        "idle action execution is not complete; use `tensor-idle --check`, `--check-wayland`, or `--observe`",
    )
    .into())
}
