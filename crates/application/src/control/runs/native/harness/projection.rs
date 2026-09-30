//! Complete native history is projected into an immutable suffix before moving the Session ref.
use std::collections::{BTreeMap, HashMap, HashSet};

use ait_contracts::ApiError;
use ait_domain::{
    DomainMetadata, ErrorCode, LifecyclePhase, LifecycleStatus, Message, MessageId, MessageKind,
    MessageOrigin, MessageRole, ProjectId, ProviderSyncState, RunId, SessionId, SessionSource,
    TimestampMs,
};
use ait_ports::{NativeSessionOutcome, NativeSessionSnapshot};
use sha2::{Digest, Sha256};

use super::{InputState, conflict};
use crate::control::{
    conversation::{ConversationContext, MessageRecord},
    errors::{error, project_error},
};

pub(super) fn publish(
    snapshot: &NativeSessionSnapshot,
    state: &mut ConversationContext,
    index: usize,
) -> Result<(), ApiError> {
    let session = &state.sessions[index];
    if !matches!(&session.source,SessionSource::NativeSession(source) if source.driver==snapshot.driver
        && source.native_session_id==snapshot.id)
        || std::path::Path::new(&session.workdir) != snapshot.cwd
    {
        return Err(error(
            ErrorCode::RunRecoveryFailed,
            "native history binding changed",
            false,
        ));
    }
    let root = state
        .projects
        .iter()
        .find(|project| project.id == session.project_id)
        .ok_or_else(conflict)?
        .root_message_id
        .clone();
    let mut head = MessageId::parse(&root).map_err(|_| conflict())?;
    let existing = state
        .messages
        .iter()
        .map(|message| (message.id.as_str(), message))
        .collect::<HashMap<_, _>>();
    let mut messages = Vec::new();
    let inputs = input_index(snapshot, &state.runs)?;
    let mut runs = state.runs.clone();
    let mut active = None::<usize>;
    let mut seq = 0;
    let mut seen = HashSet::<String>::new();
    for native in &snapshot.messages {
        if let Some(input) = &native.input_id {
            if !seen.insert(input.clone()) {
                return Err(error(
                    ErrorCode::RunRecoveryFailed,
                    "native input correlation is ambiguous",
                    false,
                ));
            }
            active = inputs.get(input.as_str()).copied();
            seq = 0;
            if let Some(run) = active.map(|index| &runs[index]) {
                validate_input(native, run, head)?;
            }
        } else if native.role == MessageRole::User && native.tool_result.is_none() {
            active = None;
            seq = 0;
        }
        if active.is_some() {
            seq += 1;
        }
        let record = materialize(
            snapshot,
            native,
            session,
            head,
            active.map(|index| (&runs[index], seq)),
        )?;
        let id = MessageId::parse(&record.id).map_err(|_| conflict())?;
        if let Some(previous) = existing.get(record.id.as_str()) {
            if *previous != &record {
                return Err(error(
                    ErrorCode::MessageImmutable,
                    "native history would alter an immutable Message",
                    false,
                ));
            }
        } else {
            messages.push(record);
        }
        head = id;
        if let Some(index) = active {
            runs[index].set_last_message_id(Some(id.to_string()));
        }
    }
    if let Some(index) = active
        && let Some(outcome) = snapshot.outcome
    {
        settle(&mut runs[index], outcome, seq)?;
    }
    let mut session = session.clone();
    session
        .reference
        .reconcile(session.reference.head(), session.version(), head)
        .map_err(project_error)?;
    if let SessionSource::NativeSession(source) = &mut session.source {
        source.sync_state = ProviderSyncState::Synced;
    }
    state.sessions[index] = session;
    state.messages.extend(messages);
    state.runs = runs;
    Ok(())
}

#[cfg(test)]
mod tests;

fn materialize(
    snapshot: &NativeSessionSnapshot,
    native: &ait_ports::NativeHistoryMessage,
    session: &crate::control::conversation::SessionRecord,
    head: MessageId,
    provenance: Option<(&super::RunRecord, u64)>,
) -> Result<MessageRecord, ApiError> {
    let bytes = serde_json::to_vec(&(head, native)).map_err(|_| conflict())?;
    let digest = Sha256::digest(bytes);
    let mut identity = [0u8; 16];
    identity.copy_from_slice(&digest[..16]);
    let id = MessageId::new(uuid::Uuid::from_bytes(identity));
    let mut metadata = BTreeMap::new();
    metadata.insert(snapshot.driver.clone(), native.metadata.clone());
    let message = Message {
        id,
        project_id: ProjectId::new(&session.project_id),
        parent_message_id: Some(head),
        role: native.role,
        kind: if native.tool_result.is_some() {
            MessageKind::ToolResult
        } else {
            MessageKind::Standard
        },
        origin: if native.tool_result.is_some() {
            MessageOrigin::Tool
        } else {
            MessageOrigin::Provider
        },
        sub_messages: native.sub_messages.clone(),
        created_by_session_id: Some(SessionId::new(&session.id)),
        run_id: provenance.map(|(run, _)| RunId::new(&run.id)),
        run_seq: provenance.map(|(_, seq)| seq),
        tool_result: native.tool_result.clone(),
        git_commit: None,
        metadata: DomainMetadata(metadata),
        created_at: TimestampMs(native.created_at),
    };
    message.validate().map_err(|_| {
        error(
            ErrorCode::RunRecoveryFailed,
            "native history violates the Message protocol",
            false,
        )
    })?;
    Ok(MessageRecord::from(message))
}

fn settle(
    run: &mut super::RunRecord,
    outcome: NativeSessionOutcome,
    seq: u64,
) -> Result<(), ApiError> {
    let stopped = matches!(
        run.status(),
        LifecycleStatus::Cancelled | LifecycleStatus::LimitExceeded
    );
    let input = run.harness_input.as_mut().ok_or_else(conflict)?;
    if input.state != InputState::Published
        && seq > 0
        && (seq > 1 || outcome != NativeSessionOutcome::Completed)
    {
        input.state = InputState::Published;
        if stopped {
            return Ok(());
        }
        run.set_status(match outcome {
            NativeSessionOutcome::Completed | NativeSessionOutcome::Interrupted
                if run.status() == LifecycleStatus::Cancelling =>
            {
                LifecycleStatus::Cancelled
            }
            NativeSessionOutcome::Completed => LifecycleStatus::Completed,
            NativeSessionOutcome::Failed => LifecycleStatus::Failed,
            NativeSessionOutcome::Interrupted => LifecycleStatus::Interrupted,
        });
        run.set_phase(Some(LifecyclePhase::Terminal));
        run.set_error(if outcome == NativeSessionOutcome::Completed {
            None
        } else {
            Some(error(
                ErrorCode::ProviderFailed,
                "native provider execution did not complete",
                false,
            ))
        });
    }
    Ok(())
}

fn validate_input(
    native: &ait_ports::NativeHistoryMessage,
    run: &super::RunRecord,
    head: MessageId,
) -> Result<(), ApiError> {
    let text = native
        .sub_messages
        .iter()
        .filter_map(|part| {
            if let ait_domain::SubMessage::Text { text } = part {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if run.base_message_id != head.to_string()
        || run
            .harness_input
            .as_ref()
            .is_none_or(|input| input.text != text)
    {
        return Err(error(
            ErrorCode::RunRecoveryFailed,
            "native input no longer extends its admitted Run base",
            false,
        ));
    }
    Ok(())
}

fn input_index<'a>(
    snapshot: &NativeSessionSnapshot,
    runs: &'a [super::RunRecord],
) -> Result<HashMap<&'a str, usize>, ApiError> {
    let mut inputs = HashMap::new();
    for (position, run) in runs.iter().enumerate() {
        if let Some(input) = &run.harness_input
            && input.driver == snapshot.driver
            && input.session_id == snapshot.id
            && inputs.insert(input.input_id.as_str(), position).is_some()
        {
            return Err(error(
                ErrorCode::RunRecoveryFailed,
                "duplicate admitted native input",
                false,
            ));
        }
    }
    Ok(inputs)
}
