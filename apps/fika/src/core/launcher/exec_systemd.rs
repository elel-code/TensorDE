pub fn exec_to_launch_commands(
    exec: &str,
    app_name: &str,
    desktop_file: &Path,
    paths: &[PathBuf],
) -> Option<Vec<DesktopLaunchCommand>> {
    let argv = split_exec_line(exec)?;
    let program = argv.first()?.clone();
    let mut args = Vec::new();
    let mut file_code_used = false;
    for token in argv.into_iter().skip(1) {
        match token.as_str() {
            "%F" | "%U" => {
                file_code_used = true;
                args.extend(paths.iter().map(|path| path.display().to_string()));
            }
            _ => {
                if let Some(argument) = expand_exec_token(&token, app_name, desktop_file, paths) {
                    if exec_token_contains_file_code(&token) {
                        file_code_used = true;
                    }
                    args.push(argument);
                }
            }
        }
    }

    if !file_code_used && paths.is_empty() {
        return Some(vec![DesktopLaunchCommand { program, args }]);
    }
    Some(vec![DesktopLaunchCommand { program, args }])
}

fn desktop_launch_plan_for_exec(
    desktop_id: String,
    desktop_file: PathBuf,
    app_name: String,
    exec: &str,
    paths: &[PathBuf],
) -> Option<DesktopLaunchPlan> {
    let commands = exec_to_launch_commands(exec, &app_name, &desktop_file, paths)?;
    Some(DesktopLaunchPlan {
        desktop_id,
        desktop_file,
        app_name,
        commands,
    })
}

fn terminal_launch_plan_for_commands(
    commands: Vec<DesktopLaunchCommand>,
) -> Result<DesktopLaunchPlan, LauncherError> {
    let Some(command) = commands
        .into_iter()
        .find(|command| executable_path_for_systemd(&command.program).is_ok())
    else {
        return Err(LauncherError::TerminalNotFound);
    };
    Ok(DesktopLaunchPlan {
        desktop_id: "fika-terminal".to_string(),
        desktop_file: PathBuf::from("fika-terminal"),
        app_name: "Terminal".to_string(),
        commands: vec![command],
    })
}

fn terminal_launch_commands_for_directory(directory: &Path) -> Vec<DesktopLaunchCommand> {
    let directory = directory.display().to_string();
    vec![
        terminal_command("konsole", ["--workdir", directory.as_str()]),
        terminal_command(
            "gnome-terminal",
            [format!("--working-directory={directory}")],
        ),
        terminal_command("kgx", [format!("--working-directory={directory}")]),
        terminal_command("tilix", [format!("--working-directory={directory}")]),
        terminal_command(
            "xfce4-terminal",
            [format!("--working-directory={directory}")],
        ),
        terminal_command(
            "mate-terminal",
            [format!("--working-directory={directory}")],
        ),
        terminal_command("foot", ["--working-directory", directory.as_str()]),
        terminal_command("alacritty", ["--working-directory", directory.as_str()]),
        terminal_command("kitty", ["--directory", directory.as_str()]),
        terminal_command("wezterm", ["start", "--cwd", directory.as_str()]),
        terminal_command(
            "xterm",
            [
                "-e",
                "sh",
                "-lc",
                "cd \"$1\" && exec \"${SHELL:-sh}\"",
                "sh",
                directory.as_str(),
            ],
        ),
    ]
}

fn terminal_command(
    program: impl Into<String>,
    args: impl IntoIterator<Item = impl Into<String>>,
) -> DesktopLaunchCommand {
    DesktopLaunchCommand {
        program: program.into(),
        args: args.into_iter().map(Into::into).collect(),
    }
}

fn systemd_units_for_launch_plan_with_nonce(
    plan: &DesktopLaunchPlan,
    nonce: u128,
) -> Result<Vec<SystemdLaunchUnit>, LauncherError> {
    if plan.commands.is_empty() {
        return Err(LauncherError::EmptyLaunchPlan {
            app_name: plan.app_name.clone(),
        });
    }

    plan.commands
        .iter()
        .enumerate()
        .map(|(index, command)| {
            let command = systemd_launch_command(command, &plan.app_name)?;
            Ok(SystemdLaunchUnit {
                unit_name: systemd_launch_unit_name(&plan.desktop_id, index, nonce),
                description: format!("Fika Open With {}", plan.app_name),
                command,
            })
        })
        .collect()
}

fn systemd_launch_command(
    command: &DesktopLaunchCommand,
    app_name: &str,
) -> Result<DesktopLaunchCommand, LauncherError> {
    if command.program.trim().is_empty() {
        return Err(LauncherError::EmptyCommand {
            app_name: app_name.to_string(),
        });
    }
    let program = executable_path_for_systemd(&command.program)?;
    Ok(DesktopLaunchCommand {
        program: program.display().to_string(),
        args: command.args.clone(),
    })
}

pub fn systemd_launch_unit_name(desktop_id: &str, index: usize, nonce: u128) -> String {
    let component = sanitize_systemd_unit_component(desktop_id);
    format!("fika-open-with-{component}-{index}-{nonce:x}.service")
}

fn sanitize_systemd_unit_component(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len().min(48));
    for ch in value.chars() {
        let next = if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            ch
        } else {
            '-'
        };
        if sanitized.ends_with('-') && next == '-' {
            continue;
        }
        sanitized.push(next);
        if sanitized.len() >= 48 {
            break;
        }
    }
    let sanitized = sanitized.trim_matches('-').trim_matches('.').to_string();
    if sanitized.is_empty() {
        "application".to_string()
    } else {
        sanitized
    }
}

fn systemd_launch_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn executable_path_for_systemd(program: &str) -> Result<PathBuf, LauncherError> {
    let program_path = Path::new(program);
    if program_path.is_absolute() {
        return executable_file_path(program_path).ok_or_else(|| LauncherError::ProgramNotFound {
            program: program.to_string(),
        });
    }
    if program.contains('/') {
        return Err(LauncherError::ProgramNotFound {
            program: program.to_string(),
        });
    }

    for dir in env::var_os("PATH")
        .filter(|path| !path.is_empty())
        .map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ]
        })
    {
        let candidate = dir.join(program);
        if let Some(path) = executable_file_path(&candidate) {
            return Ok(path);
        }
    }

    Err(LauncherError::ProgramNotFound {
        program: program.to_string(),
    })
}

fn executable_file_path(path: &Path) -> Option<PathBuf> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    if executable_permissions(&metadata) {
        Some(path.to_path_buf())
    } else {
        None
    }
}

#[cfg(unix)]
fn executable_permissions(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable_permissions(_metadata: &fs::Metadata) -> bool {
    true
}

type SystemdProperty = (String, OwnedValue);
type SystemdAuxUnit = (String, Vec<SystemdProperty>);
type SystemdExecCommand = (String, Vec<String>, bool);

fn systemd_properties_for_launch_unit(
    unit: &SystemdLaunchUnit,
) -> Result<Vec<SystemdProperty>, LauncherError> {
    let mut argv = Vec::with_capacity(unit.command.args.len() + 1);
    argv.push(unit.command.program.clone());
    argv.extend(unit.command.args.iter().cloned());
    let exec_start: Vec<SystemdExecCommand> = vec![(unit.command.program.clone(), argv, false)];

    let mut properties = vec![
        systemd_property("Description", unit.description.clone())?,
        systemd_property("Type", "exec".to_string())?,
        systemd_property("ExecStart", exec_start)?,
    ];
    let environment = systemd_launch_environment();
    if !environment.is_empty() {
        properties.push(systemd_property("Environment", environment)?);
    }
    Ok(properties)
}

fn systemd_property<T>(name: &'static str, value: T) -> Result<SystemdProperty, LauncherError>
where
    T: zbus::zvariant::DynamicType + Into<Value<'static>>,
{
    let value = OwnedValue::try_from(Value::new(value)).map_err(|err| {
        LauncherError::InvalidSystemdProperty {
            property: name,
            message: err.to_string(),
        }
    })?;
    Ok((name.to_string(), value))
}

fn systemd_launch_environment() -> Vec<String> {
    const KEYS: &[&str] = &[
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_TYPE",
        "DBUS_SESSION_BUS_ADDRESS",
        "SSH_AUTH_SOCK",
        "LANG",
        "LC_ALL",
    ];
    KEYS.iter()
        .filter_map(|key| env::var(key).ok().map(|value| format!("{key}={value}")))
        .collect()
}

fn systemd_manager_target() -> Result<BusCallTarget, BusError> {
    BusCallTarget::new(
        BusKind::Session,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
        "StartTransientUnit",
    )
}

async fn start_systemd_launch_unit(
    bus: &BusController,
    target: &BusCallTarget,
    manager: &zbus::Proxy<'_>,
    unit: &SystemdLaunchUnit,
) -> Result<OwnedObjectPath, LauncherError> {
    let properties = systemd_properties_for_launch_unit(unit)?;
    let aux: Vec<SystemdAuxUnit> = Vec::new();
    bus.call_with_retry(target, || {
        let properties = properties.clone();
        let aux = aux.clone();
        async move {
            manager
                .call(
                    target.method(),
                    &(unit.unit_name.as_str(), "fail", properties, aux),
                )
                .await
        }
    })
    .await
    .map_err(|err| LauncherError::StartTransientUnit {
        unit_name: unit.unit_name.clone(),
        message: err.to_string(),
    })
}

