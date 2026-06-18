use super::protocol::PROTOCOL_VERSION;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub enum ClientCommand {
    Ping,
    Status,
    Outputs,
    Watch,
    PropertiesGet {
        output: Option<String>,
        key: Option<String>,
    },
    PropertiesSet {
        output: Option<String>,
        key: String,
        value: Value,
    },
    PropertiesUnset {
        output: Option<String>,
        key: String,
    },
    Set {
        wallpaper: String,
        output: Option<String>,
        variant: Option<String>,
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
        let request = match self {
            Self::Ping => json_request("ping", json!({ "protocol": PROTOCOL_VERSION })),
            Self::Status => json_request("status", json!({})),
            Self::Outputs => json_request("outputs", json!({})),
            Self::Watch => json_request("watch", json!({ "include_snapshot": true })),
            Self::PropertiesGet { output, key } => json_request(
                "properties.get",
                json!({
                    "output": output,
                    "key": key,
                }),
            ),
            Self::PropertiesSet { output, key, value } => json_request(
                "properties.set",
                json!({
                    "output": output,
                    "key": key,
                    "value": value,
                }),
            ),
            Self::PropertiesUnset { output, key } => json_request(
                "properties.unset",
                json!({
                    "output": output,
                    "key": key,
                }),
            ),
            Self::Set {
                wallpaper,
                output,
                variant,
            } => json_request(
                "set",
                json!({
                    "wallpaper": wallpaper,
                    "output": output,
                    "variant": variant,
                }),
            ),
            Self::Pause { output } => json_request("pause", json!({ "output": output })),
            Self::Resume { output } => json_request("resume", json!({ "output": output })),
            Self::Stop { output } => json_request("stop", json!({ "output": output })),
        };
        serde_json::to_string(&request).expect("IPC request should serialize")
    }
}

pub fn parse_client_args(args: &[String]) -> Result<ClientCommand, String> {
    match args {
        [cmd] if cmd == "ping" => Ok(ClientCommand::Ping),
        [cmd] if cmd == "status" => Ok(ClientCommand::Status),
        [cmd] if cmd == "outputs" => Ok(ClientCommand::Outputs),
        [cmd] if cmd == "watch" => Ok(ClientCommand::Watch),
        [cmd, sub] if cmd == "properties" && sub == "get" => Ok(ClientCommand::PropertiesGet {
            output: None,
            key: None,
        }),
        [cmd, sub, key] if cmd == "properties" && sub == "get" => {
            Ok(ClientCommand::PropertiesGet {
                output: None,
                key: Some(key.clone()),
            })
        }
        [cmd, sub, flag, output] if cmd == "properties" && sub == "get" && flag == "--output" => {
            Ok(ClientCommand::PropertiesGet {
                output: Some(output.clone()),
                key: None,
            })
        }
        [cmd, sub, key, flag, output]
            if cmd == "properties" && sub == "get" && flag == "--output" =>
        {
            Ok(ClientCommand::PropertiesGet {
                output: Some(output.clone()),
                key: Some(key.clone()),
            })
        }
        [cmd, sub, key, value] if cmd == "properties" && sub == "set" => {
            Ok(ClientCommand::PropertiesSet {
                output: None,
                key: key.clone(),
                value: parse_cli_value(value),
            })
        }
        [cmd, sub, key, value, flag, output]
            if cmd == "properties" && sub == "set" && flag == "--output" =>
        {
            Ok(ClientCommand::PropertiesSet {
                output: Some(output.clone()),
                key: key.clone(),
                value: parse_cli_value(value),
            })
        }
        [cmd, sub, key] if cmd == "properties" && sub == "unset" => {
            Ok(ClientCommand::PropertiesUnset {
                output: None,
                key: key.clone(),
            })
        }
        [cmd, sub, key, flag, output]
            if cmd == "properties" && sub == "unset" && flag == "--output" =>
        {
            Ok(ClientCommand::PropertiesUnset {
                output: Some(output.clone()),
                key: key.clone(),
            })
        }
        [cmd, wallpaper] if cmd == "set" => Ok(ClientCommand::Set {
            wallpaper: wallpaper.clone(),
            output: None,
            variant: None,
        }),
        [cmd, wallpaper, flag, output] if cmd == "set" && flag == "--output" => {
            Ok(ClientCommand::Set {
                wallpaper: wallpaper.clone(),
                output: Some(output.clone()),
                variant: None,
            })
        }
        [cmd, wallpaper, flag, variant] if cmd == "set" && flag == "--variant" => {
            Ok(ClientCommand::Set {
                wallpaper: wallpaper.clone(),
                output: None,
                variant: Some(variant.clone()),
            })
        }
        [cmd, wallpaper, output_flag, output, variant_flag, variant]
            if cmd == "set" && output_flag == "--output" && variant_flag == "--variant" =>
        {
            Ok(ClientCommand::Set {
                wallpaper: wallpaper.clone(),
                output: Some(output.clone()),
                variant: Some(variant.clone()),
            })
        }
        [cmd, wallpaper, variant_flag, variant, output_flag, output]
            if cmd == "set" && variant_flag == "--variant" && output_flag == "--output" =>
        {
            Ok(ClientCommand::Set {
                wallpaper: wallpaper.clone(),
                output: Some(output.clone()),
                variant: Some(variant.clone()),
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
        "  gilderctl status --decisions-csv [--from-file <status.json>]",
        "  gilderctl status --telemetry-csv [--from-file <status.json>]",
        "  gilderctl outputs",
        "  gilderctl watch",
        "  gilderctl properties get [key] [--output <name>]",
        "  gilderctl properties set <key> <value-json> [--output <name>]",
        "  gilderctl properties unset <key> [--output <name>]",
        "  gilderctl set <wallpaper.gwp|wallpaper.gwpdir> [--output <name>] [--variant <id>]",
        "  gilderctl pause [--output <name>]",
        "  gilderctl resume [--output <name>]",
        "  gilderctl stop [--output <name>]",
    ]
    .join("\n")
}

fn json_request(method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    })
}

fn parse_cli_value(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()))
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
                output: Some("eDP-1".to_owned()),
                variant: None,
            })
        );
    }

    #[test]
    fn parses_set_with_output_and_variant() {
        let args = vec![
            "set".to_owned(),
            "wall.gwp".to_owned(),
            "--output".to_owned(),
            "eDP-1".to_owned(),
            "--variant".to_owned(),
            "uhd".to_owned(),
        ];
        assert_eq!(
            parse_client_args(&args),
            Ok(ClientCommand::Set {
                wallpaper: "wall.gwp".to_owned(),
                output: Some("eDP-1".to_owned()),
                variant: Some("uhd".to_owned()),
            })
        );
    }

    #[test]
    fn escapes_json_strings() {
        let cmd = ClientCommand::Set {
            wallpaper: "a\"b\\c".to_owned(),
            output: None,
            variant: Some("wide".to_owned()),
        };
        assert!(cmd.to_json_line().contains(r#""wallpaper":"a\"b\\c""#));
        assert!(cmd.to_json_line().contains(r#""variant":"wide""#));
    }

    #[test]
    fn parses_property_set_with_json_value() {
        let args = vec![
            "properties".to_owned(),
            "set".to_owned(),
            "speed".to_owned(),
            "0.5".to_owned(),
            "--output".to_owned(),
            "eDP-1".to_owned(),
        ];
        assert_eq!(
            parse_client_args(&args),
            Ok(ClientCommand::PropertiesSet {
                output: Some("eDP-1".to_owned()),
                key: "speed".to_owned(),
                value: Value::from(0.5)
            })
        );
    }

    #[test]
    fn parses_property_set_plain_value_as_string() {
        let args = vec![
            "properties".to_owned(),
            "set".to_owned(),
            "accent".to_owned(),
            "#ffaa00".to_owned(),
        ];
        assert_eq!(
            parse_client_args(&args),
            Ok(ClientCommand::PropertiesSet {
                output: None,
                key: "accent".to_owned(),
                value: Value::String("#ffaa00".to_owned())
            })
        );
    }

    #[test]
    fn parses_watch() {
        let args = vec!["watch".to_owned()];
        assert_eq!(parse_client_args(&args), Ok(ClientCommand::Watch));
    }
}
