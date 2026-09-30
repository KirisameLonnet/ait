//! In-memory native provider with durable admission assertions at the external effect boundary.
use ait_domain::{DomainError, ErrorCode, MessageRole, SubMessage};
use ait_ports::{
    ControlFilter, ControlRecordKind, ControlStore, NativeHistoryMessage, NativeSessionConnection,
    NativeSessionInvocation, NativeSessionOutcome, NativeSessionSnapshot, NativeSessionWriter,
    WorkspaceProgressReporter,
};
use async_trait::async_trait;
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

pub(super) struct Writer {
    pub(super) requests: Arc<Mutex<Vec<NativeSessionInvocation>>>,
    pub(super) close_count: Arc<AtomicUsize>,
    pub(super) start_count: Arc<AtomicUsize>,
    history: Arc<Mutex<Vec<NativeHistoryMessage>>>,
    pub(super) store: Arc<dyn ControlStore>,
}
impl Writer {
    pub(super) fn new(store: Arc<dyn ControlStore>) -> Self {
        Self {
            requests: Arc::default(),
            close_count: Arc::default(),
            start_count: Arc::default(),
            history: Arc::default(),
            store,
        }
    }
}
struct Connection {
    request: NativeSessionInvocation,
    prepared: NativeSessionSnapshot,
    history: Arc<Mutex<Vec<NativeHistoryMessage>>>,
    close_count: Arc<AtomicUsize>,
    start_count: Arc<AtomicUsize>,
    store: Arc<dyn ControlStore>,
}
#[async_trait]
impl NativeSessionWriter for Writer {
    async fn open(
        &self,
        request: NativeSessionInvocation,
    ) -> Result<Box<dyn NativeSessionConnection>, DomainError> {
        self.requests.lock().unwrap().push(request.clone());
        let history = self.history.lock().unwrap().clone();
        Ok(Box::new(Connection {
            prepared: NativeSessionSnapshot {
                driver: request.driver.clone(),
                id: "ses_one".into(),
                input_id: request.input_id.clone(),
                cwd: request.cwd.clone(),
                model: request.model.clone(),
                reasoning_effort: request.reasoning_effort.clone(),
                outcome: (!history.is_empty()).then_some(NativeSessionOutcome::Completed),
                messages: history,
            },
            request,
            history: self.history.clone(),
            close_count: self.close_count.clone(),
            start_count: self.start_count.clone(),
            store: self.store.clone(),
        }))
    }
}
#[async_trait]
impl NativeSessionConnection for Connection {
    fn prepared(&self) -> &NativeSessionSnapshot {
        &self.prepared
    }
    async fn start(
        &mut self,
        _: Arc<dyn WorkspaceProgressReporter>,
    ) -> Result<NativeSessionSnapshot, DomainError> {
        self.start_count.fetch_add(1, Ordering::SeqCst);
        let read = self
            .store
            .read(&[ControlFilter::id(
                ControlRecordKind::Run,
                &self.request.request_id,
            )])
            .await
            .unwrap();
        assert_eq!(read.records.len(), 1);
        let run = &read.records[0].value;
        assert_eq!(run["harness_input"]["state"], "send_unknown");
        assert_eq!(run["harness_input"]["input_id"], self.request.input_id);
        assert_eq!(run["provider"]["kind"], "opencode");
        if self.request.prompt == "panic" {
            panic!("simulated executor panic");
        }
        let mut history = self.history.lock().unwrap();
        history.extend([
            NativeHistoryMessage {
                id: format!("user-{}", self.request.input_id),
                role: MessageRole::User,
                input_id: Some(self.request.input_id.clone()),
                sub_messages: vec![SubMessage::Text {
                    text: self.request.prompt.clone(),
                }],
                tool_result: None,
                created_at: 1,
                metadata: json!({}),
            },
            NativeHistoryMessage {
                id: format!("answer-{}", self.request.input_id),
                role: MessageRole::Assistant,
                input_id: None,
                sub_messages: vec![SubMessage::Text {
                    text: "answer".into(),
                }],
                tool_result: None,
                created_at: 2,
                metadata: json!({}),
            },
        ]);
        if self.request.prompt == "tools" {
            history
                .last_mut()
                .unwrap()
                .sub_messages
                .push(SubMessage::ToolUse(ait_domain::ToolUse {
                    call_id: "call1".into(),
                    tool_name: "shell".into(),
                    arguments: json!({"command":"pwd"}).to_string(),
                    provider_metadata: None,
                }));
            history.push(NativeHistoryMessage {
                id: "result-call1".into(),
                role: MessageRole::User,
                input_id: None,
                sub_messages: vec![],
                tool_result: Some(ait_domain::ToolResult {
                    call_id: "call1".into(),
                    status: ait_domain::ToolResultStatus::Succeeded,
                    output: Some("/tmp".into()),
                    error: None,
                }),
                created_at: 3,
                metadata: json!({}),
            });
        }
        if self.request.prompt == "early failure" {
            history.pop();
        }
        if self.request.prompt == "lost response" {
            return Err(DomainError::invariant(
                ErrorCode::RunRecoveryFailed,
                "input outcome unknown",
            ));
        }
        let mut result = self.prepared.clone();
        result.messages = history.clone();
        result.outcome = Some(if self.request.prompt == "early failure" {
            NativeSessionOutcome::Failed
        } else {
            NativeSessionOutcome::Completed
        });
        Ok(result)
    }
    async fn read(&mut self) -> Result<NativeSessionSnapshot, DomainError> {
        let mut result = self.prepared.clone();
        result.messages = self.history.lock().unwrap().clone();
        Ok(result)
    }
    async fn close(&mut self) {
        self.close_count.fetch_add(1, Ordering::SeqCst);
    }
}
