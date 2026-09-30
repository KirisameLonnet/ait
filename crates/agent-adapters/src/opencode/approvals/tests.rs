use super::*;
use ait_ports::WorkspaceApproval;
use async_trait::async_trait;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Approval {
    withdraw: bool,
    entered: tokio::sync::Semaphore,
    expired: AtomicUsize,
}
#[async_trait]
impl WorkspaceApproval for Approval {
    async fn decide(
        &self,
        _: WorkspaceApprovalRequest,
    ) -> Result<WorkspaceApprovalDecision, DomainError> {
        self.entered.add_permits(1);
        if self.withdraw {
            std::future::pending().await
        } else {
            Ok(WorkspaceApprovalDecision::Approved {
                scope: ait_domain::ApprovalGrantScope::Session,
                permissions: None,
            })
        }
    }
    async fn expire(&self, _: &WorkspaceApprovalRequest) -> Result<(), DomainError> {
        self.expired.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn session_grants_reply_once_and_withdrawn_permissions_expire() {
    for withdraw in [false, true] {
        let fixture = super::super::tests::fixture::Fixture::start(Version::V2).await;
        let runtime = super::super::runtime::Runtime::spawn(
            &fixture.binary,
            &fixture.cwd,
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();
        let approval = Arc::new(Approval {
            withdraw,
            entered: tokio::sync::Semaphore::new(0),
            expired: AtomicUsize::new(0),
        });
        let mut request = super::super::tests::invocation(fixture.cwd.clone());
        request.approvals = approval.clone();
        fixture
            .state
            .lock()
            .unwrap()
            .pending_permissions
            .push(json!({"id":"perm1","sessionID":"ses_one","action":"shell","resources":["pwd"]}));
        let mut pending = Pending::new();
        pending
            .reconcile(&runtime.api, &request, "ses_one")
            .await
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            approval.entered.acquire(),
        )
        .await
        .unwrap()
        .unwrap()
        .forget();
        if withdraw {
            fixture.state.lock().unwrap().pending_permissions.clear();
            pending
                .reconcile(&runtime.api, &request, "ses_one")
                .await
                .unwrap();
            assert_eq!(approval.expired.load(Ordering::SeqCst), 1);
            assert!(fixture.state.lock().unwrap().replies.is_empty());
        } else {
            tokio::time::timeout(std::time::Duration::from_secs(3), pending.receiver.recv())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(
                fixture.state.lock().unwrap().replies,
                vec![json!({"decision":"once"})]
            );
        }
        let mut runtime = runtime;
        runtime.close().await;
    }
}

#[test]
fn only_concrete_reviewable_permissions_reach_the_host() {
    let request = super::super::tests::invocation("/tmp/project".into());
    let mut data =
        json!({"id":"perm1","sessionID":"ses_one","action":"shell","resources":["git *"]});
    assert!(normalize(Version::V2, &request, "ses_one", &data).is_none());
    data["metadata"] = json!({"command":"git status"});
    let approval = normalize(Version::V2, &request, "ses_one", &data).unwrap();
    assert_eq!(approval.run_id, "run-1");
    assert_eq!(approval.protocol_request_id, json!("perm1"));
    assert!(
        matches!(approval.target, NativeApprovalTarget::Command {command,..} if command=="git status")
    );
    data["metadata"] = json!({"command":"echo bearer secret"});
    assert!(normalize(Version::V2, &request, "ses_one", &data).is_none());
    data = json!({"id":"perm2","permission":"edit","patterns":["src/main.rs"]});
    assert!(
        matches!(normalize(Version::V1, &request, "ses_one", &data).unwrap().target,
        NativeApprovalTarget::FileChange {changes,..} if changes[0].path=="/tmp/project/src/main.rs")
    );
    data["patterns"] = json!(["src/*"]);
    assert!(normalize(Version::V1, &request, "ses_one", &data).is_none());
    data = json!({"id":"perm3","action":"network","resources":["*"]});
    assert!(normalize(Version::V2, &request, "ses_one", &data).is_none());
}
