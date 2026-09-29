use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn release_cancels_an_in_flight_read_and_suppresses_its_delivery() {
    let runtime = crate::tests::runtime();
    let (outbound, mut receiver) = Outbound::new();
    let started = Arc::new(tokio::sync::Notify::new());
    let reading = started.clone();
    let subscription = Subscription::spawn(runtime.clone(), outbound, move || {
        let started = reading.clone();
        async move {
            started.notify_one();
            std::future::pending::<Result<Vec<ServerMessage>, ErrorCode>>().await
        }
    });
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .unwrap();
    drop(subscription);
    runtime.tasks.close();
    tokio::time::timeout(Duration::from_secs(2), runtime.tasks.wait())
        .await
        .unwrap();
    assert!(receiver.try_recv().is_err());
}

#[tokio::test]
async fn admission_exhaustion_retries_without_closing_the_connection() {
    let runtime = crate::tests::runtime();
    let (outbound, mut receiver) = Outbound::new();
    let failure = outbound.failure();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let subscription = Subscription::spawn(runtime.clone(), outbound, move || {
        let call = count.fetch_add(1, Ordering::SeqCst);
        async move {
            if call == 0 {
                Err(ErrorCode::ResourceExhausted)
            } else {
                Ok(vec![ServerMessage::Event {
                    method: "changed".to_owned(),
                    params: json!({}),
                }])
            }
        }
    });
    tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(!failure.is_cancelled());
    drop(subscription);
    runtime.tasks.close();
    runtime.tasks.wait().await;
}

#[tokio::test]
async fn failed_observation_closes_the_transport_instead_of_silently_staling() {
    let runtime = crate::tests::runtime();
    let (outbound, _receiver) = Outbound::new();
    let failure = outbound.failure();
    let subscription = Subscription::spawn(runtime.clone(), outbound, || async {
        Err(ErrorCode::RegistryIo)
    });
    tokio::time::timeout(Duration::from_secs(2), failure.cancelled())
        .await
        .unwrap();
    drop(subscription);
    runtime.tasks.close();
    runtime.tasks.wait().await;
}

#[tokio::test]
async fn server_shutdown_stops_observations_before_the_first_tick() {
    let runtime = crate::tests::runtime();
    let (outbound, _receiver) = Outbound::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let subscription = Subscription::spawn(runtime.clone(), outbound, move || {
        count.fetch_add(1, Ordering::SeqCst);
        async { Ok(Vec::new()) }
    });
    runtime.cancellation.cancel();
    runtime.tasks.close();
    tokio::time::timeout(Duration::from_secs(2), runtime.tasks.wait())
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(subscription);
}

#[tokio::test]
async fn no_task_can_be_admitted_after_runtime_shutdown() {
    let runtime = crate::tests::runtime();
    runtime.cancellation.cancel();
    runtime.tasks.close();
    let (outbound, _receiver) = Outbound::new();
    let subscription = Subscription::spawn(runtime.clone(), outbound, || async {
        panic!("closed runtime must not invoke a new observer");
    });
    assert!(runtime.tasks.is_empty());
    assert!(subscription.cancellation.is_cancelled());
}
