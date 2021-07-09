use log::Level;
use serde::{Deserialize, Serialize};

use crate::{payloads::*, rpc, Plugin};

/// A wrapper for log::Level because it doesn't implement Serialize/Deserialize.
#[derive(Serialize, Deserialize, Debug)]
pub enum LogSeverity {
    Debug,
    Info,
    Warn,
    Error,
    Trace,
}

impl From<LogSeverity> for Level {
    fn from(severity: LogSeverity) -> Self {
        match severity {
            LogSeverity::Debug => Self::Debug,
            LogSeverity::Info => Self::Info,
            LogSeverity::Warn => Self::Warn,
            LogSeverity::Error => Self::Error,
            LogSeverity::Trace => Self::Trace,
        }
    }
}

pub struct PluginLogger;

impl log::Log for PluginLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let severity = match record.level() {
                Level::Debug => LogSeverity::Debug,
                Level::Info => LogSeverity::Info,
                Level::Warn => LogSeverity::Warn,
                Level::Error => LogSeverity::Error,
                Level::Trace => LogSeverity::Trace,
            };

            let rpc_message: rpc::Message = LogPayload {
                severity,
                content: record.args().to_string(),
            }
            .into();
            Plugin::send(&rpc_message)
        }
    }

    fn flush(&self) {}
}
