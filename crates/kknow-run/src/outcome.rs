use std::sync::atomic::AtomicBool;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "RunEnd"))]
pub enum RunEnd {
    Exited,
    TimedOut,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "RunOutcome"))]
pub struct Outcome {
    pub command: String,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub end: RunEnd,
    pub truncated: bool,
    pub millis: u64,
}

impl Outcome {
    pub fn ok(&self) -> bool {
        self.end == RunEnd::Exited && self.code == Some(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "RunStream"))]
pub enum Stream {
    Stdout,
    Stderr,
}

pub struct Watch<'a> {
    pub line: &'a (dyn Fn(Stream, &str) + Sync),
    pub stop: &'a AtomicBool,
}
