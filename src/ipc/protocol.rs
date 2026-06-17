use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct IpcRequest {
    pub id: Value,
    pub method: RequestMethod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestMethod {
    Ping {
        protocol: Option<u32>,
    },
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
struct SetParams {
    wallpaper: String,
    output: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OutputParams {
    output: Option<String>,
}

pub(super) fn optional_json_string(value: Option<&str>) -> String {
    match value {
        Some(value) => format!(r#""{}""#, escape_json_string(value)),
        None => "null".to_owned(),
    }
}

fn escape_json_string(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
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
}
