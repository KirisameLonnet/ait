use super::*;

#[test]
fn workspace_closure_is_scoped_by_identity_and_retains_failed_processes_for_retry() {
    let (mut service, registry, calls) = fixture();
    let mut other = registry.workspaces.lock().unwrap()[0].clone();
    other.workspace_id = "other".into();
    registry.workspaces.lock().unwrap().push(other);
    let mut input = request();
    input.workspace_id = Some("w".into());
    let first = service.create(&input).unwrap();
    input.workspace_id = Some("other".into());
    let second = service.create(&input).unwrap();
    calls.lock().unwrap().failure = true;
    assert_eq!(service.close_workspaces(&["w".into()]), Err(Error::Io));
    assert!(!service.entries[&first.id].closed);
    calls.lock().unwrap().failure = false;
    service
        .close_workspaces(&["w".into(), "w".into(), "missing".into()])
        .unwrap();
    service.close_workspaces(&["w".into()]).unwrap();
    assert_eq!(calls.lock().unwrap().killed, 1);
    assert!(service.entries[&first.id].closed);
    assert!(!service.entries[&second.id].closed);
}
