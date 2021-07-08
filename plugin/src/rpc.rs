use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum Id {
    Str(String),
    Int(i32)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcError {
    code: i32,
    message: String,
    data: Option<Value>
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} (code {})", self.message, self.code)
    }
}

impl Error for RpcError {}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Message {
    Notification { jsonrpc: String, method: String, params: Option<Value> },
    Request { jsonrpc: String, id: Id, method: String, params: Option<Value> },
    Response { jsonrpc: String, id: Id, result: Option<Value>, error: Option<RpcError> }
}

impl Message {
    pub fn notification(method: &str, params: Option<Value>) -> Self {
        Message::Notification { jsonrpc: "2.0".into(), method: method.into(), params }
    }

    pub fn request(id: Id, method: &str, params: Option<Value>) -> Self {
        Message::Request { jsonrpc: "2.0".into(), id, method: method.into(), params }
    }

    pub fn response(id: Id, result: Option<Value>, error: Option<RpcError>) -> Self {
        Message::Response { jsonrpc: "2.0".into(), id, result, error }
    }

    /// Gets an Option<&str> representing the method of the message. Some for Notifications/Requests, None for Responses.
    pub fn method(&self) -> Option<&str> {
        match self {
            Message::Notification { method, .. } => Some(method.as_str()),
            Message::Request { method, .. } => Some(method.as_str()),
            Message::Response { .. } => None
        }
    }
}
