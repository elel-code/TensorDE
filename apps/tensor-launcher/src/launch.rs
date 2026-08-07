use tensor_ipc::land::{Command, CompioClient, ResultBody};

use crate::DesktopEntry;

const TERMINAL_EXEC: &str = "xdg-terminal-exec";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchPlan {
    pub desktop_id: String,
    pub argv: Vec<String>,
    pub working_directory: Option<String>,
}

impl LaunchPlan {
    pub fn for_entry(entry: &DesktopEntry) -> Result<Self, LaunchError> {
        let mut argv = split_exec(&entry.exec)?;
        expand_field_codes(entry, &mut argv)?;
        if argv.is_empty() || argv[0].is_empty() {
            return Err(LaunchError::EmptyCommand {
                desktop_id: entry.id.clone(),
            });
        }
        if entry.terminal {
            argv.insert(0, TERMINAL_EXEC.to_owned());
        }
        Ok(Self {
            desktop_id: entry.id.clone(),
            argv,
            working_directory: entry
                .working_directory
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
        })
    }
}

pub struct LauncherClient {
    client: CompioClient,
}

impl LauncherClient {
    pub async fn connect() -> Result<Self, LaunchError> {
        Ok(Self {
            client: CompioClient::connect_default().await?,
        })
    }

    pub fn from_client(client: CompioClient) -> Self {
        Self { client }
    }

    pub async fn submit(&mut self, plan: LaunchPlan) -> Result<(), LaunchError> {
        match self
            .client
            .call(Command::Spawn {
                argv: plan.argv,
                cwd: plan.working_directory,
            })
            .await?
        {
            ResultBody::Accepted => Ok(()),
            _ => Err(LaunchError::UnexpectedResponse),
        }
    }
}

fn split_exec(exec: &str) -> Result<Vec<String>, LaunchError> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in exec.chars() {
        if character.is_control() && !character.is_whitespace() {
            return Err(LaunchError::InvalidExec("control character"));
        }
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            character if character.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    argv.push(std::mem::take(&mut current));
                }
            }
            character => current.push(character),
        }
    }
    if escaped || quoted {
        return Err(LaunchError::InvalidExec("unterminated quote or escape"));
    }
    if !current.is_empty() {
        argv.push(current);
    }
    if argv.is_empty() {
        return Err(LaunchError::InvalidExec("empty Exec value"));
    }
    Ok(argv)
}

fn expand_field_codes(entry: &DesktopEntry, argv: &mut Vec<String>) -> Result<(), LaunchError> {
    let mut expanded = Vec::with_capacity(argv.len().saturating_add(1));
    for token in argv.drain(..) {
        if token == "%i" {
            if let Some(icon) = &entry.icon {
                expanded.push("--icon".to_owned());
                expanded.push(icon.clone());
            }
            continue;
        }
        if token == "%F" || token == "%U" {
            continue;
        }
        if (token.contains("%F") || token.contains("%U") || token.contains("%i"))
            && token.len() != 2
        {
            return Err(LaunchError::InvalidExec(
                "multi-argument field code must occupy one argument",
            ));
        }
        let token = expand_token(entry, &token)?;
        if !token.is_empty() {
            expanded.push(token);
        }
    }
    *argv = expanded;
    Ok(())
}

fn expand_token(entry: &DesktopEntry, token: &str) -> Result<String, LaunchError> {
    let mut output = String::with_capacity(token.len());
    let mut characters = token.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            output.push(character);
            continue;
        }
        match characters.next() {
            Some('%') => output.push('%'),
            Some('c') => output.push_str(&entry.name),
            Some('k') => {
                if let Some(path) = &entry.desktop_file {
                    output.push_str(&path.to_string_lossy());
                }
            }
            Some('f' | 'u' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm') => {}
            Some(code) => return Err(LaunchError::UnknownFieldCode(code)),
            None => return Err(LaunchError::InvalidExec("trailing percent field code")),
        }
    }
    Ok(output)
}

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("invalid desktop Exec value: {0}")]
    InvalidExec(&'static str),
    #[error("unknown desktop Exec field code %{0}")]
    UnknownFieldCode(char),
    #[error("desktop entry {desktop_id} expands to an empty command")]
    EmptyCommand { desktop_id: String },
    #[error(transparent)]
    Ipc(#[from] tensor_ipc::land::ClientError),
    #[error("Tensor WM returned a non-acceptance response to a launch request")]
    UnexpectedResponse,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LauncherCatalog;

    fn entry(exec: &str, icon: Option<&str>, terminal: bool) -> DesktopEntry {
        let source = format!(
            "[Desktop Entry]\nType=Application\nName=Demo App\nExec={exec}\nIcon={}\nTerminal={}\n",
            icon.unwrap_or_default(),
            if terminal { "true" } else { "false" }
        );
        LauncherCatalog::parse_entry("demo.desktop", &source)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn exec_is_split_without_a_shell_and_expands_single_value_codes() {
        let mut entry = entry("demo --name=\"%c\" %% %i %f", Some("demo-icon"), false);
        entry.desktop_file = Some("/usr/share/applications/demo.desktop".into());
        let plan = LaunchPlan::for_entry(&entry).unwrap();
        assert_eq!(
            plan.argv,
            ["demo", "--name=Demo App", "%", "--icon", "demo-icon"]
        );
    }

    #[test]
    fn terminal_entries_use_the_standard_terminal_launcher() {
        let entry = entry("demo --interactive", None, true);
        let plan = LaunchPlan::for_entry(&entry).unwrap();
        assert_eq!(plan.argv[0], TERMINAL_EXEC);
        assert_eq!(&plan.argv[1..], ["demo", "--interactive"]);
    }

    #[test]
    fn desktop_working_directory_is_preserved_for_tensor_wm() {
        let mut entry = entry("demo", None, false);
        entry.working_directory = Some("/srv/demo".into());
        let plan = LaunchPlan::for_entry(&entry).unwrap();
        assert_eq!(plan.working_directory.as_deref(), Some("/srv/demo"));
    }

    #[test]
    fn malformed_exec_and_unknown_codes_are_rejected() {
        assert!(LaunchPlan::for_entry(&entry("demo \"bad", None, false)).is_err());
        assert!(matches!(
            LaunchPlan::for_entry(&entry("demo %Z", None, false)),
            Err(LaunchError::UnknownFieldCode('Z'))
        ));
    }
}
