//! Provider-neutral worker operations; native protocol details stay in plugins.
use serde::{Deserialize, Serialize};

/// An owned coding-agent runtime operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    /// Prepare a session without sending input; retain it through Start/Read/Close.
    Open {
        /// Stable registered plugin identifier.
        driver: String,
        /// Host Run identity for approval routing.
        request_id: String,
        /// Provider session to resume; absent creates one.
        session_id: Option<String>,
        /// Input correlation, independent of host Message identities.
        input_id: String,
        /// New user input only.
        prompt: String,
        /// Instructions installed only on a new session.
        instructions: Option<String>,
        /// Native model identifier.
        model: String,
        /// Native reasoning variant.
        reasoning_effort: Option<String>,
    },
    /// Discover connected host-authenticated models.
    Models {
        /// Stable registered plugin identifier.
        driver: String,
    },
}

/// Provider-neutral commands issued only after durable native input admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Send the durably admitted input once.
    Start,
    /// Reread authoritative history without replaying input.
    Read,
    /// Release and reap the writer before completing the worker.
    Close,
}
