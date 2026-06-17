use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct IpcRequest {
    pub id: Value,
    pub method: RequestMethod,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RequestMethod {
    Ping {
        protocol: Option<u32>,
    },
    Status,
    Outputs,
    Watch {
        include_snapshot: bool,
    },
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

#[derive(Debug, Clone, PartialEq)]
pub struct RpcError {
    pub id: Option<Value>,
    pub code: &'static str,
    pub message: String,
}

impl RpcError {
    fn parse(message: impl Into<String>) -> Self {
        Self {
            id: None,
            code: "bad_request",
            message: message.into(),
        }
    }

    fn invalid_request(id: Option<Value>, message: impl Into<String>) -> Self {
        Self {
            id,
            code: "bad_request",
            message: message.into(),
        }
    }

    fn invalid_params(
        id: Value,
        method: &str,
        source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        Self {
            id: Some(id),
            code: "bad_request",
            message: format!("invalid params for {method}: {}", source.into()),
        }
    }

    fn unknown_method(id: Value, method: String) -> Self {
        Self {
            id: Some(id),
            code: "not_found",
            message: format!("unknown IPC method {method:?}"),
        }
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RpcError {}

pub fn parse_request(line: &str) -> Result<IpcRequest, RpcError> {
    let envelope: JsonRpcRequest =
        serde_json::from_str(line).map_err(|err| RpcError::parse(err.to_string()))?;
    if envelope.jsonrpc != "2.0" {
        return Err(RpcError::invalid_request(
            Some(envelope.id),
            "jsonrpc must be \"2.0\"",
        ));
    }

    let id = envelope.id;
    let method = match envelope.method.as_str() {
        "ping" => {
            let params: PingParams = parse_params(id.clone(), &envelope.method, envelope.params)?;
            RequestMethod::Ping {
                protocol: params.protocol,
            }
        }
        "status" => RequestMethod::Status,
        "outputs" => RequestMethod::Outputs,
        "watch" => {
            let params: WatchParams = parse_params(id.clone(), &envelope.method, envelope.params)?;
            RequestMethod::Watch {
                include_snapshot: params.include_snapshot,
            }
        }
        "properties.get" => {
            let params: PropertiesGetParams =
                parse_params(id.clone(), &envelope.method, envelope.params)?;
            RequestMethod::PropertiesGet {
                output: params.output,
                key: params.key,
            }
        }
        "properties.set" => {
            let params: PropertiesSetParams =
                parse_params(id.clone(), &envelope.method, envelope.params)?;
            RequestMethod::PropertiesSet {
                output: params.output,
                key: params.key,
                value: params.value,
            }
        }
        "properties.unset" => {
            let params: PropertiesUnsetParams =
                parse_params(id.clone(), &envelope.method, envelope.params)?;
            RequestMethod::PropertiesUnset {
                output: params.output,
                key: params.key,
            }
        }
        "set" => {
            let params: SetParams = parse_params(id.clone(), &envelope.method, envelope.params)?;
            RequestMethod::Set {
                wallpaper: params.wallpaper,
                output: params.output,
            }
        }
        "pause" => {
            let params: OutputParams = parse_params(id.clone(), &envelope.method, envelope.params)?;
            RequestMethod::Pause {
                output: params.output,
            }
        }
        "resume" => {
            let params: OutputParams = parse_params(id.clone(), &envelope.method, envelope.params)?;
            RequestMethod::Resume {
                output: params.output,
            }
        }
        "stop" => {
            let params: OutputParams = parse_params(id.clone(), &envelope.method, envelope.params)?;
            RequestMethod::Stop {
                output: params.output,
            }
        }
        _ => return Err(RpcError::unknown_method(id, envelope.method)),
    };

    Ok(IpcRequest { id, method })
}

pub fn success_response(id: &Value, result: Value) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    }))
    .expect("JSON-RPC success response should serialize")
}

pub fn error_response(id: Option<&Value>, code: &str, message: &str) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id.cloned().unwrap_or(Value::Null),
        "error": {
            "code": code,
            "message": message,
        },
    }))
    .expect("JSON-RPC error response should serialize")
}

pub fn event_notification(sequence: u64, event_type: &str, payload: Value) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "sequence": sequence,
            "type": event_type,
            "payload": payload,
        },
    }))
    .expect("JSON-RPC event notification should serialize")
}

fn parse_params<T: for<'de> Deserialize<'de>>(
    id: Value,
    method: &str,
    params: Value,
) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(|err| RpcError::invalid_params(id, method, err))
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
struct PingParams {
    protocol: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct WatchParams {
    #[serde(default = "default_true")]
    include_snapshot: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct SetParams {
    wallpaper: String,
    output: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PropertiesGetParams {
    output: Option<String>,
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PropertiesSetParams {
    output: Option<String>,
    key: String,
    value: Value,
}

#[derive(Debug, Deserialize)]
struct PropertiesUnsetParams {
    output: Option<String>,
    key: String,
}

#[derive(Debug, Deserialize)]
struct OutputParams {
    output: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_set_request() {
        let request = parse_request(
            r#"{"jsonrpc":"2.0","id":7,"method":"set","params":{"wallpaper":"demo.gwpdir","output":"eDP-1"}}"#,
        )
        .unwrap();
        assert_eq!(request.id, json!(7));
        assert_eq!(
            request.method,
            RequestMethod::Set {
                wallpaper: "demo.gwpdir".to_owned(),
                output: Some("eDP-1".to_owned())
            }
        );
    }

    #[test]
    fn rejects_unknown_method() {
        let error =
            parse_request(r#"{"jsonrpc":"2.0","id":3,"method":"bogus","params":{}}"#).unwrap_err();
        assert_eq!(error.id, Some(json!(3)));
        assert_eq!(error.code, "not_found");
    }

    #[test]
    fn builds_json_rpc_error_response() {
        let response = error_response(Some(&json!(1)), "bad_request", "invalid");
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["error"]["code"], "bad_request");
    }

    #[test]
    fn parses_property_set_request() {
        let request = parse_request(
            r##"{"jsonrpc":"2.0","id":9,"method":"properties.set","params":{"output":"eDP-1","key":"accent","value":"#ffaa00"}}"##,
        )
        .unwrap();
        assert_eq!(
            request.method,
            RequestMethod::PropertiesSet {
                output: Some("eDP-1".to_owned()),
                key: "accent".to_owned(),
                value: Value::String("#ffaa00".to_owned())
            }
        );
    }

    #[test]
    fn parses_watch_request() {
        let request = parse_request(
            r#"{"jsonrpc":"2.0","id":11,"method":"watch","params":{"include_snapshot":false}}"#,
        )
        .unwrap();
        assert_eq!(
            request.method,
            RequestMethod::Watch {
                include_snapshot: false
            }
        );
    }

    #[test]
    fn builds_event_notification() {
        let response = event_notification(3, "state.changed", json!({ "action": "set" }));
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["method"], "event");
        assert_eq!(value["params"]["sequence"], 3);
        assert_eq!(value["params"]["type"], "state.changed");
    }
}
