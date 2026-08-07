use std::{env, process::ExitCode};

use tensor_idle::{
    IdleConfig, IdleConfigWatcher, IdleMonitorRuntime, IdlePlan, LogindActionExecutor, PowerSource,
    PowerSourceService, PowerSourceStatus, system_actions_required,
};

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
    let mut config_watcher = IdleConfigWatcher::start(IdleConfig::resolve_path())?;
    let mut config = config_watcher.config().clone();
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let check = arguments.iter().any(|argument| argument == "--check");
    let check_wayland = arguments
        .iter()
        .any(|argument| argument == "--check-wayland");
    let observe = arguments.iter().any(|argument| argument == "--observe");
    let run_output_power = arguments
        .iter()
        .any(|argument| argument == "--run-output-power");
    let run_all_actions =
        arguments.is_empty() || arguments.iter().any(|argument| argument == "--run");
    let battery = arguments.iter().any(|argument| argument == "--battery");
    let mut source = if battery {
        PowerSource::Battery
    } else {
        PowerSource::Ac
    };
    let mut plan = IdlePlan::compile(&config, source);
    if check && !check_wayland && !observe && !run_output_power && !run_all_actions {
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
            "tensor-idle: registered {} {source:?} idle notifications and {} output-power controls",
            runtime.monitor_count(),
            runtime.output_power_count()
        );
        return Ok(());
    }
    if observe || run_output_power || run_all_actions {
        let mut runtime = IdleMonitorRuntime::connect(&plan)?;
        let power_source = (!battery)
            .then(|| PowerSourceService::start(runtime.wake_handle()))
            .transpose()?;
        let mut power_generation = 0;
        let any_system_actions = [PowerSource::Ac, PowerSource::Battery]
            .into_iter()
            .map(|source| IdlePlan::compile(&config, source))
            .any(|plan| system_actions_required(&plan));
        let system_runtime = (run_all_actions && any_system_actions)
            .then(|| tensor_runtime::io_uring_runtime(64))
            .transpose()?;
        let mut system_actions = match &system_runtime {
            Some(system_runtime) if run_all_actions => {
                Some(system_runtime.block_on(LogindActionExecutor::connect())?)
            }
            _ => None,
        };
        let mut transitions = Vec::with_capacity(plan.stages.len());
        loop {
            transitions.clear();
            runtime.dispatch_into(Some(std::time::Duration::from_secs(1)), &mut transitions)?;
            match config_watcher.reload_if_changed() {
                Ok(Some(next_config)) => {
                    let next_plan = IdlePlan::compile(&next_config, source);
                    let next_system_actions = [PowerSource::Ac, PowerSource::Battery]
                        .into_iter()
                        .map(|candidate| IdlePlan::compile(&next_config, candidate))
                        .any(|candidate| system_actions_required(&candidate));
                    if next_system_actions != system_runtime.is_some() {
                        eprintln!(
                            "tensor-idle: live change adds or removes logind actions; restart required"
                        );
                        let _ = config_watcher.restore(config.clone());
                    } else if let Err(error) = runtime.reconfigure(&next_plan) {
                        eprintln!("tensor-idle: retaining policy after reload failure: {error}");
                        let _ = config_watcher.restore(config.clone());
                    } else {
                        config = next_config;
                        plan = next_plan;
                        transitions.reserve(plan.stages.len());
                        println!("tensor-idle: configuration reloaded");
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    eprintln!("tensor-idle: retaining policy after KDL reload failure: {error}")
                }
            }
            if let Some(service) = &power_source {
                let (generation, status) = service.read();
                if generation != power_generation {
                    power_generation = generation;
                    match status {
                        PowerSourceStatus::Ready(next_source) if next_source != source => {
                            let next_plan = IdlePlan::compile(&config, next_source);
                            runtime.reconfigure(&next_plan)?;
                            source = next_source;
                            plan = next_plan;
                            transitions.reserve(plan.stages.len());
                            println!("tensor-idle: power source changed to {source:?}");
                        }
                        PowerSourceStatus::Unavailable => {
                            eprintln!(
                                "tensor-idle: UPower unavailable; retaining {source:?} policy"
                            );
                        }
                        PowerSourceStatus::Failed => {
                            eprintln!(
                                "tensor-idle: UPower observation failed; retaining {source:?} policy"
                            );
                        }
                        PowerSourceStatus::Pending | PowerSourceStatus::Ready(_) => {}
                    }
                }
            }
            for &transition in &transitions {
                let mut executed = (run_output_power || run_all_actions)
                    && runtime.apply_monitor_power_transition(transition)?;
                if let (Some(system_runtime), Some(system_actions)) =
                    (&system_runtime, &mut system_actions)
                {
                    let system_executed =
                        system_runtime.block_on(system_actions.apply_transition(transition))?;
                    if system_executed
                        && transition.action == tensor_idle::IdleAction::Lock
                        && transition.idle
                    {
                        runtime.rebase_post_lock_monitor(&plan)?;
                    }
                    executed |= system_executed;
                }
                println!(
                    "tensor-idle: action={:?} idle={} after_ms={} executed={executed}",
                    transition.action, transition.idle, transition.after_ms,
                );
            }
        }
    }
    unreachable!("every command mode returns or enters the service loop")
}
