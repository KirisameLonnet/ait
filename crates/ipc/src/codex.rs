//! Codex ports backed exclusively by supervised worker processes.
use std::sync::{Arc, Mutex};

use ait_contracts::worker::{
    Bootstrap, Executor, Lease, ProtocolError, StoreRequest, StoreResponse,
    codex::{Action, MAX_RESULT_BYTES, Operation},
};
use ait_domain::{AgentProvider, DomainError, ErrorCode, ProviderModel, RunPermissionProfile};
use ait_ports::{
    CodexHistorySource, CodexPreparedThread, CodexThreadConnection, CodexThreadInvocation,
    CodexThreadSnapshot, CodexThreadSourceKind, CodexThreadWriter, GeneratedSessionTitle,
    HostProviderModelCatalog, SessionTitleGenerator, SessionTitleRequest, WorkspaceApproval,
    WorkspaceProgressReporter,
};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::{Mutex as AsyncMutex, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    mapping::Wire,
    supervisor::{Handler, WorkerSupervisor},
};

type Reply = Result<(Value, bool), DomainError>;

struct Context {
    project_execution: Option<Arc<dyn ait_ports::ProjectExecution>>,
    request_id: String,
    approvals: Arc<dyn WorkspaceApproval>,
    cancellation: CancellationToken,
}

fn failed() -> DomainError {
    DomainError::invariant(
        ErrorCode::CodexInputOutcomeUnknown,
        "Codex worker disconnected; reconcile native history before sending more input",
    )
}

#[derive(Default)]
struct Assembly {
    bytes: Vec<u8>,
    total: usize,
    closed: bool,
}

struct Server {
    native: bool,
    project_execution: Option<Arc<dyn ait_ports::ProjectExecution>>,
    lease: Lease,
    request_id: Option<String>,
    assembly: Mutex<Assembly>,
    actions: AsyncMutex<mpsc::Receiver<Action>>,
    replies: mpsc::Sender<Reply>,
    progress: Arc<Mutex<Option<Arc<dyn WorkspaceProgressReporter>>>>,
    approvals: Option<Arc<dyn WorkspaceApproval>>,
    cancel: CancellationToken,
}

impl Server {
    fn assemble(
        &self,
        offset: usize,
        total: usize,
        bytes: &[u8],
    ) -> Result<Option<Reply>, ProtocolError> {
        let mut assembly = self.assembly.lock().map_err(|_| ProtocolError::Io)?;
        if assembly.closed
            || total == 0
            || total > MAX_RESULT_BYTES
            || offset != assembly.bytes.len()
            || bytes.is_empty()
            || bytes.len() > ait_contracts::worker::codex::CHUNK_BYTES
            || offset
                .checked_add(bytes.len())
                .is_none_or(|end| end > total)
            || (offset > 0 && total != assembly.total)
        {
            return Err(ProtocolError::InvalidFrame);
        }
        assembly.total = total;
        assembly.bytes.extend_from_slice(bytes);
        if assembly.bytes.len() == total {
            let reply = serde_json::from_slice::<Reply>(&assembly.bytes)
                .map_err(|_| ProtocolError::InvalidFrame)?;
            assembly.bytes.clear();
            Ok(Some(reply))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl Handler for Server {
    async fn spawned(&self, pid: u32) -> Result<(), ProtocolError> {
        if let Some(project) = &self.project_execution {
            project
                .register_process(pid)
                .await
                .map_err(|_| ProtocolError::StaleWorkerLease)?;
        }
        Ok(())
    }
    async fn reaped(&self, pid: u32) -> Result<(), ProtocolError> {
        if let Some(project) = &self.project_execution {
            project
                .release_process(pid)
                .await
                .map_err(|_| ProtocolError::StaleWorkerLease)?;
        }
        Ok(())
    }
    fn capacity(&self) -> usize {
        16
    }

    fn disconnected(&self) {
        self.cancel.cancel();
    }

    async fn request(
        &self,
        lease: &Lease,
        _operation_id: &str,
        request: StoreRequest,
    ) -> Result<StoreResponse, ProtocolError> {
        if lease != &self.lease {
            return Err(ProtocolError::StaleWorkerLease);
        }
        match request {
            StoreRequest::CodexChunk {
                offset,
                total,
                bytes,
            }
            | StoreRequest::NativeChunk {
                offset,
                total,
                bytes,
            } => {
                let result = self.assemble(offset, total, &bytes)?;
                if let Some(reply) = result {
                    self.replies
                        .send(reply)
                        .await
                        .map_err(|_| ProtocolError::Io)?;
                }
                Ok(StoreResponse::Unit)
            }
            StoreRequest::CodexNext | StoreRequest::NativeNext => {
                let mut receiver = self.actions.lock().await;
                let action = tokio::select! {
                    action = receiver.recv() => action.unwrap_or(Action::Close),
                    () = self.cancel.cancelled() => Action::Close,
                };
                Ok(if self.native {
                    StoreResponse::NativeAction { action }
                } else {
                    StoreResponse::CodexAction { action }
                })
            }
            StoreRequest::CodexClosed | StoreRequest::NativeClosed => {
                let mut assembly = self.assembly.lock().map_err(|_| ProtocolError::Io)?;
                if !assembly.bytes.is_empty() {
                    return Err(ProtocolError::InvalidTransition);
                }
                assembly.closed = true;
                Ok(StoreResponse::Unit)
            }
            StoreRequest::WorkspaceProgress { event } => {
                let progress = self.progress.lock().map_err(|_| ProtocolError::Io)?.clone();
                if let Some(progress) = progress {
                    progress
                        .report(ait_ports::WorkspaceProgressEvent::from_wire(*event)?)
                        .await;
                }
                Ok(StoreResponse::Unit)
            }
            StoreRequest::WorkspaceApproval { request, expire } => {
                let request = ait_ports::WorkspaceApprovalRequest::from_wire(*request)?;
                if self.request_id.as_deref() != Some(request.run_id.as_str()) {
                    return Err(ProtocolError::WrongRun);
                }
                let approvals = self
                    .approvals
                    .as_ref()
                    .ok_or(ProtocolError::InvalidTransition)?;
                if expire {
                    approvals
                        .expire(&request)
                        .await
                        .map_err(|_| ProtocolError::InvalidTransition)?;
                    Ok(StoreResponse::Unit)
                } else {
                    let decision = tokio::select! {
                        result = approvals.decide(request) => result.map_err(|_| ProtocolError::InvalidTransition)?,
                        () = self.cancel.cancelled() => return Err(ProtocolError::WorkerExited),
                    };
                    Ok(StoreResponse::WorkspaceApproval {
                        decision: decision.to_wire(),
                    })
                }
            }
            _ => Err(ProtocolError::InvalidFrame),
        }
    }

    async fn finished(&self) -> Result<(), ProtocolError> {
        if self.assembly.lock().map_err(|_| ProtocolError::Io)?.closed {
            Ok(())
        } else {
            Err(ProtocolError::InvalidTransition)
        }
    }
}

fn connection_failure(native: bool) -> DomainError {
    if native {
        DomainError::invariant(
            ErrorCode::RunRecoveryFailed,
            "native worker disconnected; reconcile history before sending more input",
        )
    } else {
        failed()
    }
}

struct RemoteConnection {
    resumed: Option<CodexPreparedThread>,
    native_prepared: Option<ait_ports::NativeSessionSnapshot>,
    native: bool,
    native_started: bool,
    actions: mpsc::Sender<Action>,
    replies: mpsc::Receiver<Reply>,
    progress: Arc<Mutex<Option<Arc<dyn WorkspaceProgressReporter>>>>,
    cancel: CancellationToken,
    worker: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for RemoteConnection {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl RemoteConnection {
    async fn receive<T: DeserializeOwned>(&mut self) -> Result<(T, bool), DomainError> {
        let (value, owned) = self
            .replies
            .recv()
            .await
            .ok_or_else(|| connection_failure(self.native))??;
        serde_json::from_value(value)
            .map(|value| (value, owned))
            .map_err(|_| connection_failure(self.native))
    }

    async fn history(&mut self, action: Action) -> Result<CodexThreadSnapshot, DomainError> {
        self.actions.send(action).await.map_err(|_| failed())?;
        let (mut history, owned): (CodexThreadSnapshot, bool) = self.receive().await?;
        history.writer_confirmed = owned;
        Ok(history)
    }
}

#[async_trait]
impl CodexThreadConnection for RemoteConnection {
    fn prepared(&self) -> &CodexPreparedThread {
        self.resumed
            .as_ref()
            .expect("writer exposed only after prepared response")
    }

    async fn start(
        &mut self,
        progress: Arc<dyn WorkspaceProgressReporter>,
    ) -> Result<CodexThreadSnapshot, DomainError> {
        *self.progress.lock().map_err(|_| failed())? = Some(progress);
        self.history(Action::Start).await
    }

    async fn read(&mut self) -> Result<CodexThreadSnapshot, DomainError> {
        self.history(Action::Read).await
    }

    async fn close(&mut self) {
        let _ = self.actions.send(Action::Close).await;
        if let Some(worker) = self.worker.take() {
            let _ = worker.await;
        }
    }
}

impl WorkerSupervisor {
    fn codex_connection(
        &self,
        operation: Operation,
        cwd: String,
        permission: RunPermissionProfile,
        invocation: Option<&CodexThreadInvocation>,
    ) -> RemoteConnection {
        let context = invocation.map(|request| Context {
            project_execution: request.project_execution.clone(),
            request_id: request.request_id.clone(),
            approvals: request.approvals.clone(),
            cancellation: request.cancellation.clone(),
        });
        self.worker_connection(
            Executor::Codex {
                binary: self.codex_binary.to_string_lossy().into_owned(),
                operation: Box::new(operation),
            },
            cwd,
            permission,
            context.as_ref(),
        )
    }

    fn worker_connection(
        &self,
        executor: Executor,
        cwd: String,
        permission: RunPermissionProfile,
        context: Option<&Context>,
    ) -> RemoteConnection {
        let native = matches!(&executor, Executor::Native { .. });
        let identity = uuid::Uuid::new_v4().to_string();
        let lease = Lease {
            project_owner: context.as_ref().and_then(|request| {
                request
                    .project_execution
                    .as_ref()
                    .map(|project| project.owner())
            }),
            scope_id: format!("native:{identity}"),
            worker_instance_id: identity,
            lease_epoch: 1,
        };
        let cancel = context
            .as_ref()
            .map_or_else(CancellationToken::new, |request| {
                request.cancellation.child_token()
            });
        let (actions, receiver) = mpsc::channel(1);
        let (sender, replies) = mpsc::channel(2);
        let progress = Arc::new(Mutex::new(None));
        let server = Server {
            native: matches!(&executor, Executor::Native { .. }),
            project_execution: context
                .as_ref()
                .and_then(|request| request.project_execution.clone()),
            lease: lease.clone(),
            request_id: context.as_ref().map(|request| request.request_id.clone()),
            assembly: Mutex::new(Assembly::default()),
            actions: AsyncMutex::new(receiver),
            replies: sender.clone(),
            progress: progress.clone(),
            approvals: context.as_ref().map(|request| request.approvals.clone()),
            cancel: cancel.clone(),
        };
        let bootstrap = Bootstrap {
            lease,
            limits: self.limits.clone(),
            workdir: cwd,
            maximum_sandbox: permission.sandbox.to_wire(),
            permission: permission.to_wire(),
            executor,
        };
        let supervisor = self.clone();
        let worker_cancel = cancel.clone();
        let worker = tokio::spawn(async move {
            if let Err(failure) = supervisor.process(bootstrap, &server, worker_cancel).await {
                let mut error = connection_failure(native);
                error.message = format!("{} ({failure})", error.message);
                if failure == ProtocolError::ResourceLimit {
                    error.code = ErrorCode::RunLimitExceeded;
                }
                let _ = sender.send(Err(error)).await;
            }
        });
        RemoteConnection {
            resumed: None,
            native_prepared: None,
            native,
            native_started: false,
            actions,
            replies,
            progress,
            cancel,
            worker: Some(worker),
        }
    }

    async fn codex_query<T: DeserializeOwned>(
        &self,
        operation: Operation,
        cwd: String,
        cancellation: CancellationToken,
    ) -> Result<T, DomainError> {
        let mut connection =
            self.codex_connection(operation, cwd, RunPermissionProfile::default(), None);
        let result = tokio::select! {
            result = connection.receive::<T>() => result.map(|(value, _)| value),
            () = cancellation.cancelled() => { connection.cancel.cancel(); Err(failed()) },
        };
        connection.close().await;
        result
    }
}

#[async_trait]
impl CodexThreadWriter for WorkerSupervisor {
    async fn open(
        &self,
        request: CodexThreadInvocation,
    ) -> Result<Box<dyn CodexThreadConnection>, DomainError> {
        let operation = Operation::Open {
            request_id: request.request_id.clone(),
            thread_id: request.thread_id.clone(),
            prompt: request.prompt.clone(),
            model: request.model.clone(),
            reasoning_effort: request.reasoning_effort.clone(),
            developer_instructions: request.developer_instructions.clone(),
        };
        let mut connection = self.codex_connection(
            operation,
            request.cwd.to_string_lossy().into_owned(),
            request.permission_profile,
            Some(&request),
        );
        match connection.receive::<CodexPreparedThread>().await {
            Ok((mut resumed, owned)) => {
                resumed.history.writer_confirmed = owned;
                connection.resumed = Some(resumed);
                Ok(Box::new(connection))
            }
            Err(error) => {
                connection.close().await;
                Err(error)
            }
        }
    }
}

fn host_cwd() -> Result<String, DomainError> {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|_| failed())
}

#[async_trait]
impl CodexHistorySource for WorkerSupervisor {
    async fn list_threads(
        &self,
        source_kinds: &[CodexThreadSourceKind],
    ) -> Result<Vec<CodexThreadSnapshot>, DomainError> {
        let source_kinds =
            serde_json::from_value(serde_json::to_value(source_kinds).map_err(|_| failed())?)
                .map_err(|_| failed())?;
        self.codex_query(
            Operation::List { source_kinds },
            host_cwd()?,
            CancellationToken::new(),
        )
        .await
    }

    async fn read_thread(&self, thread_id: &str) -> Result<CodexThreadSnapshot, DomainError> {
        self.codex_query(
            Operation::Read {
                thread_id: thread_id.into(),
            },
            host_cwd()?,
            CancellationToken::new(),
        )
        .await
    }
}

#[async_trait]
impl HostProviderModelCatalog for WorkerSupervisor {
    async fn discover_models(
        &self,
        provider: &AgentProvider,
    ) -> Result<Vec<ProviderModel>, DomainError> {
        if provider.kind == ait_domain::ProviderKind::OpenCode {
            let mut connection = self.worker_connection(
                Executor::Native {
                    binary: self.opencode_binary.to_string_lossy().into_owned(),
                    operation: Box::new(ait_contracts::worker::native::Operation::Models {
                        driver: "opencode".into(),
                    }),
                },
                host_cwd()?,
                RunPermissionProfile::default(),
                None,
            );
            let result = connection
                .receive::<Vec<ProviderModel>>()
                .await
                .map(|(models, _)| models);
            CodexThreadConnection::close(&mut connection).await;
            return result;
        }
        self.codex_query(
            Operation::Models {
                provider: provider.clone(),
            },
            host_cwd()?,
            CancellationToken::new(),
        )
        .await
    }
}

#[async_trait]
impl ait_ports::NativeSessionConnection for RemoteConnection {
    fn prepared(&self) -> &ait_ports::NativeSessionSnapshot {
        self.native_prepared
            .as_ref()
            .expect("plugin connection exposed only after preparation")
    }
    async fn start(
        &mut self,
        progress: Arc<dyn WorkspaceProgressReporter>,
    ) -> Result<ait_ports::NativeSessionSnapshot, DomainError> {
        if self.native_started {
            return Err(DomainError::invariant(
                ErrorCode::RunRecoveryFailed,
                "native input must not be replayed",
            ));
        }
        self.native_started = true;
        *self.progress.lock().map_err(|_| connection_failure(true))? = Some(progress);
        self.actions
            .send(Action::Start)
            .await
            .map_err(|_| connection_failure(true))?;
        let (history, owned) = self.receive::<ait_ports::NativeSessionSnapshot>().await?;
        if !owned {
            return Err(connection_failure(true));
        }
        Ok(history)
    }
    async fn read(&mut self) -> Result<ait_ports::NativeSessionSnapshot, DomainError> {
        self.actions
            .send(Action::Read)
            .await
            .map_err(|_| connection_failure(true))?;
        let (history, owned) = self.receive::<ait_ports::NativeSessionSnapshot>().await?;
        if !owned {
            return Err(connection_failure(true));
        }
        Ok(history)
    }
    async fn close(&mut self) {
        CodexThreadConnection::close(self).await;
    }
}

#[async_trait]
impl ait_ports::NativeSessionWriter for WorkerSupervisor {
    async fn open(
        &self,
        request: ait_ports::NativeSessionInvocation,
    ) -> Result<Box<dyn ait_ports::NativeSessionConnection>, DomainError> {
        if request.driver != "opencode" {
            return Err(DomainError::invariant(
                ErrorCode::AgentCapabilityUnsupported,
                "native plugin is not registered",
            ));
        }
        let operation = ait_contracts::worker::native::Operation::Open {
            driver: request.driver.clone(),
            request_id: request.request_id.clone(),
            session_id: request.session_id.clone(),
            input_id: request.input_id.clone(),
            prompt: request.prompt.clone(),
            instructions: request.instructions.clone(),
            model: request.model.clone(),
            reasoning_effort: request.reasoning_effort.clone(),
        };
        let context = Context {
            project_execution: request.project_execution,
            request_id: request.request_id,
            approvals: request.approvals,
            cancellation: request.cancellation,
        };
        let mut connection = self.worker_connection(
            Executor::Native {
                binary: self.opencode_binary.to_string_lossy().into_owned(),
                operation: Box::new(operation),
            },
            request.cwd.to_string_lossy().into_owned(),
            request.permission_profile,
            Some(&context),
        );
        let response = connection
            .receive::<ait_ports::NativeSessionSnapshot>()
            .await;
        match response {
            Ok((prepared, true)) => {
                connection.native_prepared = Some(prepared);
                Ok(Box::new(connection))
            }
            Ok((_, false)) => {
                CodexThreadConnection::close(&mut connection).await;
                Err(connection_failure(true))
            }
            Err(error) => {
                CodexThreadConnection::close(&mut connection).await;
                Err(error)
            }
        }
    }
}

#[async_trait]
impl SessionTitleGenerator for WorkerSupervisor {
    async fn generate(
        &self,
        request: SessionTitleRequest,
    ) -> Result<GeneratedSessionTitle, DomainError> {
        self.codex_query(
            Operation::Title {
                request_id: request.request_id,
                user_prompt: request.user_prompt,
                config: request.config,
                provider: request.provider,
            },
            request.cwd.to_string_lossy().into_owned(),
            request.cancellation,
        )
        .await
    }
}

#[cfg(test)]
mod tests;
