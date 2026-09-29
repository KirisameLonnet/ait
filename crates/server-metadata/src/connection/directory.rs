//! Workspace directory subscriptions owned by one physical connection.

use std::sync::{Arc, Mutex};

use server_model::outbound::QueueError;
use server_model::{Context, ErrorCode};

use crate::dispatch::State;
use crate::protocol::directory::WorkspaceListRequest;

pub(crate) async fn subscribe(
    mut context: Context<'_>,
    state: &State,
    connection: &mut super::Connection,
) -> Result<(), QueueError> {
    if context.available_subscriptions == 0 {
        return context.respond(Err(ErrorCode::ResourceExhausted));
    }
    let params = std::mem::take(&mut context.request.params);
    let request: WorkspaceListRequest = match serde_json::from_value(params) {
        Ok(request) => request,
        Err(_) => return context.respond(Err(ErrorCode::InvalidMessage)),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let observer_id = id.clone();
    let prepared = state
        .runtime
        .run(
            state.directory.clone(),
            ErrorCode::RegistryIo,
            move |directory| {
                crate::rpc::directory::listing::prepare(directory, request, observer_id)
                    .map_err(Into::into)
            },
        )
        .await;
    let (value, observation) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => return context.respond(Err(error)),
    };
    let outbound = context.outbound.clone();
    context.respond(Ok(value))?;
    let observation = Arc::new(Mutex::new(observation));
    let directory = state.directory.clone();
    let runtime = state.runtime.clone();
    let subscription =
        server_model::polling::Subscription::spawn(runtime.clone(), outbound, move || {
            let runtime = runtime.clone();
            let directory = directory.clone();
            let observation = observation.clone();
            async move {
                runtime
                    .run(directory, ErrorCode::RegistryIo, move |directory| {
                        observation
                            .lock()
                            .map_err(|_| ErrorCode::RegistryIo)?
                            .update(directory)
                            .map_err(Into::into)
                    })
                    .await
            }
        });
    connection.directories.insert(id, subscription);
    Ok(())
}
