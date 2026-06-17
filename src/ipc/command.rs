use super::protocol::{optional_json_string, PROTOCOL_VERSION};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientCommand {
    Ping,
    Status,
    Set {
        wallpaper: String,
        output: Option<String>,
    },
    Pause {
        output: Option<String>,
    },
    Resume {
        output: Option<String>,
    },
    Stop {
        output: Option<String>,
    },
}

impl ClientCommand {
    pub fn to_json_line(&self) -> String {
        match self {
            Self::Ping => format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"ping","params":{{"protocol":{}}}}}"#,
                PROTOCOL_VERSION
            ),
            Self::Status => r#"{"jsonrpc":"2.0","id":1,"method":"status","params":{}}"#.to_owned(),
            Self::Set { wallpaper, output } => format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"set","params":{{"wallpaper":{},"output":{}}}}}"#,
                optional_json_string(Some(wallpaper)),
                optional_json_string(output.as_deref())
            ),
            Self::Pause { output } => format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"pause","params":{{"output":{}}}}}"#,
                optional_json_string(output.as_deref())
            ),
            Self::Resume { output } => format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"resume","params":{{"output":{}}}}}"#,
                optional_json_string(output.as_deref())
            ),
            Self::Stop { output } => format!(
                r#"{{"jsonrpc":"2.0","id":1,"method":"stop","params":{{"output":{}}}}}"#,
                optional_json_string(output.as_deref())
            ),
        }
    }
}

pub fn parse_client_args(args: &[String]) -> Result<ClientCommand, String> {
    match args {
        [cmd] if cmd == "ping" => Ok(ClientCommand::Ping),
        [cmd] if cmd == "status" => Ok(ClientCommand::Status),
        [cmd, wallpaper] if cmd == "set" => Ok(ClientCommand::Set {
            wallpaper: wallpaper.clone(),
            output: None,
        }),
        [cmd, wallpaper, flag, output] if cmd == "set" && flag == "--output" => {
            Ok(ClientCommand::Set {
                wallpaper: wallpaper.clone(),
                output: Some(output.clone()),
            })
        }
        [cmd] if cmd == "pause" => Ok(ClientCommand::Pause { output: None }),
        [cmd, flag, output] if cmd == "pause" && flag == "--output" => Ok(ClientCommand::Pause {
            output: Some(output.clone()),
        }),
        [cmd] if cmd == "resume" => Ok(ClientCommand::Resume { output: None }),
        [cmd, flag, output] if cmd == "resume" && flag == "--output" => Ok(ClientCommand::Resume {
            output: Some(output.clone()),
        }),
        [cmd] if cmd == "stop" => Ok(ClientCommand::Stop { output: None }),
        [cmd, flag, output] if cmd == "stop" && flag == "--output" => Ok(ClientCommand::Stop {
            output: Some(output.clone()),
        }),
        _ => Err(help_text()),
    }
}

pub fn help_text() -> String {
    [
        "usage:",
        "  gilderctl ping",
        "  gilderctl status",
        "  gilderctl set <wallpaper.gwp|wallpaper.gwpdir> [--output <name>]",
        "  gilderctl pause [--output <name>]",
        "  gilderctl resume [--output <name>]",
        "  gilderctl stop [--output <name>]",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_set_with_output() {
        let args = vec![
            "set".to_owned(),
            "wall.gwp".to_owned(),
            "--output".to_owned(),
            "eDP-1".to_owned(),
        ];
        assert_eq!(
            parse_client_args(&args),
            Ok(ClientCommand::Set {
                wallpaper: "wall.gwp".to_owned(),
                output: Some("eDP-1".to_owned())
            })
        );
    }

    #[test]
    fn escapes_json_strings() {
        let cmd = ClientCommand::Set {
            wallpaper: "a\"b\\c".to_owned(),
            output: None,
        };
        assert!(cmd.to_json_line().contains(r#""wallpaper":"a\"b\\c""#));
    }
}
