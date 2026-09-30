//! Provider plugins share durable admission and recovery without adopting Codex's schema.
use std::{sync::Arc, sync::atomic::Ordering};

use ait_contracts::{AgentMode, ApiError, Command};
use ait_domain::{
    ErrorCode, LifecyclePhase, LifecycleStatus, NativeSessionSource, ProviderSyncState,
    SessionSource,
};
use ait_ports::{
    ControlStoreError, NativeSessionConnection, NativeSessionInvocation, NativeSessionSnapshot,
};
use futures_util::FutureExt;
use serde::{Deserialize, Serialize};

use super::{
    InputState, NativeAdmission, NativeConnection, RunLifecycle, RunRecord, admission, conflict,
    run_updates,
};
use crate::control::{
    LocalControlService,
    errors::{error, project_error, store_error},
    events::pending,
    permissions::effective_permission_profile,
    runs::{finalization::RunControl, progress::ProgressPump},
};

mod projection;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(in crate::control) struct PendingInput {
    pub driver: String,
    pub session_id: String,
    pub input_id: String,
    pub text: String,
    pub state: InputState,
}

impl LocalControlService {
    pub(super) async fn admit_harness_command(
        &self,
        command: &Command,
        control: &Arc<RunControl>,
        lease: Option<&crate::control::admission::WorkspaceWriteLease>,
        derive_locked: bool,
    ) -> Result<Option<NativeAdmission>, ApiError> {
        let Some(plan) = self
            .native_plan(command, derive_locked, AgentMode::OpenCode)
            .await?
        else {
            return Ok(None);
        };
        crate::control::admission::ensure_idle(&plan.session)?;
        crate::control::conversation::messages::validate_message_text(&plan.text)?;
        let provider =
            crate::control::catalog::validate_config(&plan.loaded.original, &plan.agent.config)?
                .clone();
        let permission_profile = effective_permission_profile(
            &plan.loaded.original.settings,
            &provider,
            self.permission_limits,
        )?;
        let native = match &plan.session.source {
            SessionSource::Managed => None,
            SessionSource::NativeSession(source)
                if source.driver == "opencode" && source.provider_id == provider.id =>
            {
                Some(source)
            }
            SessionSource::NativeSession(_) | SessionSource::CodexThread(_) => {
                return Err(error(
                    ErrorCode::AgentCapabilityUnsupported,
                    "Session native provider cannot change",
                    false,
                ));
            }
        };
        if native.is_none() {
            crate::control::project::worktrees::prepare_command_session_worktrees(
                self.project_workspace.as_ref(),
                lease.cloned(),
                &plan.loaded.original,
                command,
                &mut Vec::new(),
            )
            .await?;
        }
        let (cwd, execution_lease) = self.harness_workspace(&plan, lease).await?;
        let writer = self.native_session_writer.as_ref().ok_or_else(|| {
            error(
                ErrorCode::AgentCapabilityUnsupported,
                "native plugin worker is unavailable",
                false,
            )
        })?;
        let run_id = uuid::Uuid::new_v4().to_string();
        let instructions = native
            .is_none()
            .then(|| initial_instructions(&plan))
            .flatten();
        let invocation = NativeSessionInvocation {
            driver: "opencode".into(),
            request_id: run_id.clone(),
            session_id: native.map(|source| source.native_session_id.clone()),
            input_id: run_id.clone(),
            prompt: plan.text.clone(),
            instructions,
            cwd,
            model: plan.agent.config.model.clone(),
            reasoning_effort: plan.agent.config.reasoning_effort.clone(),
            permission_profile,
            project_execution: self
                .project_execution(&plan.loaded.version, &plan.session.project_id),
            approvals: Arc::new(self.clone()),
            cancellation: control.cancellation.clone(),
        };
        let mut connection = writer.open(invocation).await.map_err(project_error)?;
        let mut run = pending_run(
            &plan,
            connection.prepared(),
            provider,
            permission_profile,
            run_id,
        );
        let result = self
            .commit_harness_admission(
                plan,
                &mut run,
                connection.as_mut(),
                (command, derive_locked),
            )
            .await;
        if let Err(error) = result {
            connection.close().await;
            return Err(error);
        }
        Ok(Some(NativeAdmission {
            run,
            connection: NativeConnection::Plugin(connection),
            execution_lease,
        }))
    }

    async fn harness_workspace(
        &self,
        plan: &admission::Plan,
        lease: Option<&crate::control::admission::WorkspaceWriteLease>,
    ) -> Result<
        (
            std::path::PathBuf,
            Option<crate::control::admission::WorkspaceWriteLease>,
        ),
        ApiError,
    > {
        let cwd = std::path::Path::new(&plan.session.workdir);
        let facts = self
            .project_workspace
            .path_facts(cwd, cwd)
            .await
            .map_err(project_error)?;
        let project = plan
            .loaded
            .original
            .projects
            .iter()
            .find(|project| project.id == plan.session.project_id)
            .ok_or_else(conflict)?;
        if lease
            .is_none_or(|lease| lease.canonical_root() != std::path::Path::new(&project.workdir))
        {
            return Err(error(
                ErrorCode::ProjectWorkspaceBusy,
                "Project lease changed during native admission",
                false,
            ));
        }
        let execution_lease = Some(
            self.project_workspace
                .acquire_lease(&facts.canonical_root)
                .await
                .map_err(project_error)?,
        );

        Ok((facts.canonical_root, execution_lease))
    }

    async fn commit_harness_admission(
        &self,
        plan: admission::Plan,
        run: &mut RunRecord,
        connection: &mut dyn NativeSessionConnection,
        intent: (&Command, bool),
    ) -> Result<(), ApiError> {
        let (command, derive_locked) = intent;
        let mut loaded = plan.loaded;
        let mut snapshot = connection.prepared().clone();
        for attempt in 0..4 {
            if attempt > 0 {
                let refreshed = self
                    .native_plan(command, derive_locked, AgentMode::OpenCode)
                    .await?
                    .ok_or_else(conflict)?;
                if refreshed.session != plan.session
                    || refreshed.agent != plan.agent
                    || refreshed.new_session != plan.new_session
                {
                    return Err(conflict());
                }
                loaded = refreshed.loaded;
                snapshot = connection.read().await.map_err(project_error)?;
            }
            let mut state = loaded.original.clone();
            let index =
                admission::stage_session(&mut state, &plan.session, &plan.agent, plan.new_session)?;
            crate::control::admission::ensure_idle(&state.sessions[index])?;
            let provider = crate::control::catalog::validate_config(&state, &run.config)?;
            if provider!=&run.provider || state.sessions[index].workdir!=snapshot.cwd.to_string_lossy()
                || effective_permission_profile(&state.settings,provider,self.permission_limits)?!=run.permission_profile
                || snapshot.driver!="opencode" || snapshot.model!=run.config.model
                || snapshot.reasoning_effort != run.config.reasoning_effort
                || run.harness_input.as_ref().is_none_or(|input| input.input_id != snapshot.input_id || input.session_id != snapshot.id)
                || state.sessions.iter().any(|session|session.id!=plan.session.id
                    && matches!(&session.source,SessionSource::NativeSession(source) if source.provider_id==provider.id && source.native_session_id==snapshot.id)) {
                return Err(error(ErrorCode::AgentCapabilityUnsupported,"native plugin admission context changed",false));
            }
            state.sessions[index].source =
                SessionSource::NativeSession(Box::new(NativeSessionSource {
                    driver: snapshot.driver.clone(),
                    provider_id: run.provider.id.clone(),
                    native_session_id: snapshot.id.clone(),
                    sync_state: ProviderSyncState::Synced,
                }));
            projection::publish(&snapshot, &mut state, index)?;
            run.base_message_id = state.sessions[index].current_message_id();
            state.sessions[index]
                .reference
                .acquire(ait_domain::RunId::new(&run.id))
                .map_err(project_error)?;
            state.runs.push(run.clone());
            let mut events = run_updates(&loaded.original.runs, &state.runs);
            events.push(pending(
                if plan.new_session {
                    "session.created"
                } else {
                    "session.updated"
                },
                Some(plan.session.id.clone()),
                &state.sessions[index].view(),
            ));
            let admission = self.admission.read().await;
            if self.draining.load(Ordering::Acquire) {
                return Err(error(ErrorCode::RunCancelled, "daemon is draining", false));
            }
            let result = self.persist_records(&loaded, &state, events).await;
            drop(admission);
            match result {
                Ok(()) => return Ok(()),
                Err(ControlStoreError::Conflict) => {}
                Err(failure) => return Err(store_error(failure)),
            }
        }
        Err(conflict())
    }

    pub(super) async fn supervise_harness_run(
        &self,
        id: &str,
        control: Arc<RunControl>,
        mut connection: Box<dyn NativeSessionConnection>,
    ) -> Result<RunRecord, ApiError> {
        let send = self.mark_harness_send(id).await;
        let run = match send {
            Ok(run) => run,
            Err(error) => {
                connection.close().await;
                let loaded = self.read_run_records(id).await?;
                let queued = loaded
                    .original
                    .runs
                    .iter()
                    .find(|run| run.id == id)
                    .ok_or_else(conflict)?;
                return self.finish_harness_run(queued, Err(error)).await;
            }
        };
        let pump = ProgressPump::start(Arc::clone(&self.store), &run);
        let result = if control.cancellation.is_cancelled() {
            Err(error(
                ErrorCode::RunCancelled,
                "native input cancelled before submission",
                false,
            ))
        } else {
            std::panic::AssertUnwindSafe(connection.start(pump.reporter()))
                .catch_unwind()
                .await
                .unwrap_or_else(|_| {
                    Err(ait_domain::DomainError::invariant(
                        ErrorCode::RunRecoveryFailed,
                        "native executor stopped after input admission",
                    ))
                })
                .map_err(project_error)
        };
        connection.close().await;
        let _ = pump.finish().await;
        self.finish_harness_run(&run, result).await
    }

    async fn mark_harness_send(&self, id: &str) -> Result<RunRecord, ApiError> {
        for _ in 0..4 {
            let loaded = self.read_run_records(id).await?;
            let mut state = loaded.original.clone();
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == id)
                .ok_or_else(conflict)?;
            let input = run.harness_input.as_mut().ok_or_else(conflict)?;
            if input.state != InputState::Queued {
                return Err(error(
                    ErrorCode::RunRecoveryFailed,
                    "native input must not be replayed",
                    false,
                ));
            }
            input.state = InputState::SendUnknown;
            if run.status() != LifecycleStatus::Cancelling {
                run.set_status(LifecycleStatus::Running);
                run.set_phase(Some(LifecyclePhase::CallingAgent));
            }
            let run = run.clone();
            match self
                .persist_records(
                    &loaded,
                    &state,
                    vec![pending("run.updated", Some(run.id.clone()), &run.view())],
                )
                .await
            {
                Ok(()) => return Ok(run),
                Err(ControlStoreError::Conflict) => {}
                Err(failure) => return Err(store_error(failure)),
            }
        }
        Err(conflict())
    }

    async fn finish_harness_run(
        &self,
        admitted: &RunRecord,
        result: Result<NativeSessionSnapshot, ApiError>,
    ) -> Result<RunRecord, ApiError> {
        for _ in 0..4 {
            let loaded = self.native_records(&admitted.project_id).await?;
            let mut state = loaded.original.clone();
            let index = state
                .runs
                .iter()
                .position(|run| run.id == admitted.id)
                .ok_or_else(conflict)?;
            let session = state
                .sessions
                .iter()
                .position(|session| Some(&session.id) == admitted.session_id.as_ref())
                .ok_or_else(conflict)?;
            if state.runs[index].status().is_terminal() {
                return Ok(state.runs[index].clone());
            }
            if state.sessions[session].active_run_id() != Some(&admitted.id) {
                return Err(conflict());
            }
            state.sessions[session]
                .reference
                .release(&ait_domain::RunId::new(&admitted.id));
            let publication = match &result {
                Ok(snapshot) => projection::publish(snapshot, &mut state, session),
                Err(error) => Err(error.clone()),
            };
            let run = &mut state.runs[index];
            if let Err(failure) = publication {
                run.set_status(if failure.code == ErrorCode::RunCancelled {
                    LifecycleStatus::Cancelled
                } else if failure.code == ErrorCode::RunLimitExceeded {
                    LifecycleStatus::LimitExceeded
                } else {
                    LifecycleStatus::Interrupted
                });
                run.set_phase(Some(LifecyclePhase::Terminal));
                run.set_error(Some(failure));
                if let SessionSource::NativeSession(source) = &mut state.sessions[session].source {
                    source.sync_state = ProviderSyncState::Pending;
                }
            } else if run
                .harness_input
                .as_ref()
                .is_none_or(|input| input.state != InputState::Published)
            {
                run.set_status(LifecycleStatus::Interrupted);
                run.set_phase(Some(LifecyclePhase::Terminal));
                run.set_error(Some(error(
                    ErrorCode::RunRecoveryFailed,
                    "native input acceptance is unknown; input was not replayed",
                    false,
                )));
                if let SessionSource::NativeSession(source) = &mut state.sessions[session].source {
                    source.sync_state = ProviderSyncState::Pending;
                }
            }
            if let Some(input) = &mut run.harness_input
                && input.state == InputState::Queued
            {
                input.state = InputState::Rejected;
            }
            crate::control::approvals::expire_pending_native_approvals(
                run,
                ait_domain::NativeApprovalStatus::Expired,
            );
            let run = run.clone();
            let mut events = run_updates(&loaded.original.runs, &state.runs);
            events.push(pending(
                "session.updated",
                Some(state.sessions[session].id.clone()),
                &state.sessions[session].view(),
            ));
            match self.persist_records(&loaded, &state, events).await {
                Ok(()) => {
                    self.notify_approval_waiters(&run.view());
                    let _ = self.store.clear_progress(&run.id).await;
                    return Ok(run);
                }
                Err(ControlStoreError::Conflict) => {}
                Err(failure) => return Err(store_error(failure)),
            }
        }
        Err(conflict())
    }

    pub(super) async fn recover_harness_run(&self, run: &RunRecord) -> Result<RunRecord, ApiError> {
        crate::control::permissions::validate_run_permission_ceiling(
            run.permission_profile,
            self.permission_limits,
        )
        .map_err(project_error)?;
        let input = run.harness_input.as_ref().ok_or_else(conflict)?;
        if input.state == InputState::Queued {
            return self
                .finish_harness_run(
                    run,
                    Err(error(
                        ErrorCode::RunRecoveryFailed,
                        "daemon stopped before native input admission; input was not replayed",
                        false,
                    )),
                )
                .await;
        }
        let loaded = self.read_run_records(&run.id).await?;
        let session = loaded
            .original
            .sessions
            .iter()
            .find(|session| Some(&session.id) == run.session_id.as_ref())
            .ok_or_else(conflict)?;
        let writer = self.native_session_writer.as_ref().ok_or_else(|| {
            error(
                ErrorCode::AgentCapabilityUnsupported,
                "native plugin unavailable for recovery",
                false,
            )
        })?;
        let mut connection = writer
            .open(NativeSessionInvocation {
                driver: input.driver.clone(),
                request_id: run.id.clone(),
                session_id: Some(input.session_id.clone()),
                input_id: input.input_id.clone(),
                prompt: input.text.clone(),
                instructions: None,
                cwd: session.workdir.clone().into(),
                model: run.config.model.clone(),
                reasoning_effort: run.config.reasoning_effort.clone(),
                permission_profile: run.permission_profile,
                project_execution: self.project_execution(&loaded.version, &run.project_id),
                approvals: Arc::new(self.clone()),
                cancellation: tokio_util::sync::CancellationToken::new(),
            })
            .await
            .map_err(project_error)?;
        let snapshot = connection.prepared().clone();
        connection.close().await;
        self.finish_harness_run(run, Ok(snapshot)).await
    }
}

fn pending_run(
    plan: &admission::Plan,
    snapshot: &NativeSessionSnapshot,
    provider: ait_domain::AgentProvider,
    permission_profile: ait_domain::RunPermissionProfile,
    run_id: String,
) -> RunRecord {
    RunRecord {
        auto_commit: None,
        compatibility_repair: false,
        codex_input: None,
        harness_input: Some(PendingInput {
            driver: "opencode".into(),
            session_id: snapshot.id.clone(),
            input_id: snapshot.input_id.clone(),
            text: plan.text.clone(),
            state: InputState::Queued,
        }),
        lifecycle: RunLifecycle::queued(),
        id: run_id,
        project_id: plan.session.project_id.clone(),
        base_message_id: plan.session.current_message_id(),
        session_id: Some(plan.session.id.clone()),
        agent_id: plan.agent.id.clone(),
        agent_revision: plan.agent.revision,
        config: plan.agent.config.clone(),
        provider,
        permission_profile,
        native_approvals: Vec::new(),
        tool_approvals: Vec::new(),
        tool_interactions: Vec::new(),
        trigger: ait_domain::RunTrigger::Manual,
        cron_id: None,
        scheduled_at: None,
        workspace_base_commit: None,
        workspace_base_index_tree: None,
        operation_id: None,
        lease_epoch: 0,
    }
}

fn initial_instructions(plan: &admission::Plan) -> Option<String> {
    plan.loaded
        .original
        .messages
        .iter()
        .find(|message| message.id == plan.session.current_message_id())
        .and_then(|message| message.text.clone())
}
