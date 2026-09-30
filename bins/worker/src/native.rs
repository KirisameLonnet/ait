//! Registered native plugins run inside the same supervised worker process tree.
use std::{path::PathBuf, sync::Arc};

use ait_agent_adapters::opencode::{OpenCodeAdapter, OpenCodeExecutionLimits};
use ait_contracts::worker::{
    Bootstrap, Executor, ProtocolError, StoreResponse,
    native::{Action, Operation},
};
use ait_ipc::mapping::Wire;
use ait_ports::{NativeSessionConnection, NativeSessionInvocation, NativeSessionWriter};
use tokio_util::sync::CancellationToken;

pub(crate) async fn run(
    bootstrap: Bootstrap,
    ports: Arc<crate::codex::Ports>,
    cancellation: CancellationToken,
) -> Result<(), ProtocolError> {
    let Executor::Native { binary, operation } = bootstrap.executor else {
        return Err(ProtocolError::InvalidFrame);
    };
    let adapter = OpenCodeAdapter::new(PathBuf::from(binary))
        .with_execution_limits(OpenCodeExecutionLimits {
            max_steps: bootstrap.limits.max_steps,
            max_tokens: bootstrap.limits.max_tokens,
            max_output_bytes: bootstrap
                .limits
                .max_codex_output_bytes
                .unwrap_or(bootstrap.limits.max_output_bytes)
                as usize,
        })
        .map_err(|_| ProtocolError::ResourceLimit)?;
    match *operation {
        Operation::Models { driver } if driver == "opencode" => {
            ports
                .result(
                    adapter.discover_models(bootstrap.workdir.into()).await,
                    false,
                )
                .await?;
        }
        Operation::Open {
            driver,
            request_id,
            session_id,
            input_id,
            prompt,
            instructions,
            model,
            reasoning_effort,
        } if driver == "opencode" => {
            if bootstrap.limits.max_cost_micros.is_some() {
                return Err(ProtocolError::ResourceLimit);
            }
            let invocation = NativeSessionInvocation {
                driver,
                request_id,
                session_id,
                input_id,
                prompt,
                instructions,
                cwd: bootstrap.workdir.into(),
                model,
                reasoning_effort,
                permission_profile: ait_domain::RunPermissionProfile::from_wire(
                    bootstrap.permission,
                )?,
                project_execution: None,
                approvals: ports.clone(),
                cancellation,
            };
            match adapter.open(invocation).await {
                Ok(connection) => writer(connection, &ports).await?,
                Err(error) => ports.result::<()>(Err(error), false).await?,
            }
        }
        Operation::Models { .. } | Operation::Open { .. } => {
            return Err(ProtocolError::InvalidFrame);
        }
    }
    Ok(())
}

async fn writer(
    mut connection: Box<dyn NativeSessionConnection>,
    ports: &Arc<crate::codex::Ports>,
) -> Result<(), ProtocolError> {
    let result = async {
        ports.result(Ok(connection.prepared()), true).await?;
        let mut sent = false;
        loop {
            let StoreResponse::NativeAction { action } = ports.next_native_action().await? else {
                return Err(ProtocolError::InvalidFrame);
            };
            let result = match action {
                Action::Start if !sent => {
                    sent = true;
                    connection.start(ports.clone()).await
                }
                Action::Start => return Err(ProtocolError::InvalidTransition),
                Action::Read => connection.read().await,
                Action::Close => return Ok(()),
            };
            ports.flush_progress().await;
            ports.result(result, true).await?;
        }
    }
    .await;
    connection.close().await;
    result
}
