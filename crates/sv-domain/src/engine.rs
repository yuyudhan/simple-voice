// FilePath: crates/sv-domain/src/engine.rs
//! Whether the Swift engine helper runs, as Settings shows it beside every on-device feature.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    /// Launched, not yet answering.
    Starting,
    /// Answered its handshake; on-device features work.
    Running,
    /// Exited or could not be launched; the app starts it again on its own after a pause.
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub state: EngineState,
    /// Why the helper is down, in one line; `None` unless `state` is `Down`.
    pub reason: Option<String>,
}

impl EngineStatus {
    pub fn starting() -> EngineStatus {
        EngineStatus {
            state: EngineState::Starting,
            reason: None,
        }
    }

    pub fn running() -> EngineStatus {
        EngineStatus {
            state: EngineState::Running,
            reason: None,
        }
    }

    pub fn down(reason: String) -> EngineStatus {
        EngineStatus {
            state: EngineState::Down,
            reason: Some(reason),
        }
    }
}

impl Default for EngineStatus {
    fn default() -> EngineStatus {
        EngineStatus::starting()
    }
}
