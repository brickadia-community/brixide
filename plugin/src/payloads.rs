use std::convert::TryFrom;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{logging::LogSeverity, rpc};

// Each of the payload types in this file should implement TryFrom<rpc::Message>, and rpc::Message should implement From<the payload type>.

/// Generic deserialization error from RPC to payload.
#[derive(Error, Debug)]
pub enum RpcDeserializationError {
    #[error("wrong RPC message type")]
    WrongRpcType,
    #[error("no payload data available")]
    NoValue,
    #[error(transparent)]
    JsonError(#[from] serde_json::Error)
}

/// A payload for sending log messages to the server.
#[derive(Serialize, Deserialize, Debug)]
pub struct LogPayload {
    pub severity: LogSeverity,
    pub content: String
}

impl From<LogPayload> for rpc::Message {
    fn from(payload: LogPayload) -> Self {
        rpc::Message::notification("log".into(), Some(serde_json::to_value(payload).unwrap()))
    }
}

impl TryFrom<rpc::Message> for LogPayload {
    type Error = RpcDeserializationError;

    fn try_from(value: rpc::Message) -> Result<Self, Self::Error> {
        match value {
            rpc::Message::Notification { params, .. } => Ok(serde_json::from_value(params.ok_or(RpcDeserializationError::NoValue)?)?),
            _ => Err(RpcDeserializationError::WrongRpcType)
        }
    }
}

/// A payload regarding a chat message that was sent.
#[derive(Serialize, Deserialize, Debug)]
pub struct ChatPayload {
    pub user: String,
    pub message: String
}

impl From<ChatPayload> for rpc::Message {
    fn from(payload: ChatPayload) -> Self {
        rpc::Message::notification("chat".into(), Some(serde_json::to_value(payload).unwrap()))
    }
}

impl TryFrom<rpc::Message> for ChatPayload {
    type Error = RpcDeserializationError;

    fn try_from(value: rpc::Message) -> Result<Self, Self::Error> {
        match value {
            rpc::Message::Notification { params, .. } => Ok(serde_json::from_value(params.ok_or(RpcDeserializationError::NoValue)?)?),
            _ => Err(RpcDeserializationError::WrongRpcType)
        }
    }
}
