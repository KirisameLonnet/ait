use super::*;

#[derive(Debug, Clone, Default)]
struct Cleanup {
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    calls: std::sync::Arc<std::sync::Mutex<Vec<Vec<String>>>>,
}

impl crate::ports::worktrees::WorktreeArchiveCleanup for Cleanup {
    fn close_workspaces(&self, ids: &[String]) -> Result<(), WorktreeError> {
        self.calls.lock().unwrap().push(ids.to_vec());
        if self.failed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(WorktreeError::Io("cleanup failed".into()));
        }
        Ok(())
    }
}

#[test]
fn archive_cleanup_failure_retains_checkout_and_retries_the_archived_workspace() {
    let workspaces = Workspaces::default();
    workspaces.records.lock().unwrap().push(workspace(
        "one",
        "/managed/hash/topic",
        "/managed/hash/topic",
        "prj",
    ));
    let managed = Managed::default();
    let mut service = service(&Projects::default(), &workspaces, &managed);
    let cleanup = Cleanup::default();
    service.set_archive_cleanup(Box::new(cleanup.clone()));
    cleanup
        .failed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let input = archive_input(ArchiveScope::Worktree);
    assert!(service.archive(&input, "archived").is_err());
    assert!(managed.state.lock().unwrap().removed.is_empty());
    cleanup
        .failed
        .store(false, std::sync::atomic::Ordering::SeqCst);
    service.archive(&input, "retried").unwrap();
    assert_eq!(managed.state.lock().unwrap().removed.len(), 1);
    assert_eq!(
        *cleanup.calls.lock().unwrap(),
        vec![vec!["one"], vec!["one"]]
    );
}

#[test]
fn prepared_archive_retains_checkout_until_resource_cleanup_and_retries_archived_ids() {
    let workspaces = Workspaces::default();
    workspaces.records.lock().unwrap().push(workspace(
        "one",
        "/managed/hash/topic",
        "/managed/hash/topic",
        "prj",
    ));
    let managed = Managed::default();
    let service = service(&Projects::default(), &workspaces, &managed);
    let input = ArchiveWorktree {
        workspace_id: Some("one".into()),
        worktree_path: None,
        ..archive_input(ArchiveScope::Workspace)
    };
    let pending = service.begin_archive(&input, "archived").unwrap();
    assert_eq!(pending.workspace_ids, ["one"]);
    assert!(
        workspaces
            .get("one")
            .unwrap()
            .unwrap()
            .archived_at
            .is_some()
    );
    assert!(managed.state.lock().unwrap().removed.is_empty());
    // A host cleanup failure leaves an archived record and the original checkout for retry.
    drop(pending);
    let retried = service.begin_archive(&input, "retried").unwrap();
    assert_eq!(retried.workspace_ids, ["one"]);
    assert!(
        service
            .finish_archive(retried)
            .unwrap()
            .workspace_ids
            .is_empty()
    );
    assert_eq!(managed.state.lock().unwrap().removed.len(), 1);
}

#[test]
fn a_new_reference_during_worktree_wide_archive_prevents_checkout_removal() {
    let workspaces = Workspaces::default();
    workspaces.records.lock().unwrap().push(workspace(
        "one",
        "/managed/hash/topic",
        "/managed/hash/topic",
        "prj",
    ));
    let managed = Managed::default();
    let service = service(&Projects::default(), &workspaces, &managed);
    let input = archive_input(ArchiveScope::Worktree);
    let pending = service.begin_archive(&input, "archived").unwrap();
    workspaces.records.lock().unwrap().push(workspace(
        "new",
        "/managed/hash/topic",
        "/managed/hash/topic",
        "prj",
    ));
    assert!(service.finish_archive(pending).is_err());
    assert!(managed.state.lock().unwrap().removed.is_empty());
    let pending = service.begin_archive(&input, "retried").unwrap();
    assert_eq!(pending.workspace_ids, ["one", "new"]);
    service.finish_archive(pending).unwrap();
    assert_eq!(managed.state.lock().unwrap().removed.len(), 1);
}
