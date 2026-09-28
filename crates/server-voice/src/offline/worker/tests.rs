use super::*;

#[test]
fn control_and_file_input_are_bounded() {
    let valid = b"{\"model\":\"SenseVoice\",\"directory\":\"/models\"}\n";
    let init = read_message::<Init>(&mut valid.as_slice())
        .unwrap()
        .unwrap();
    assert_eq!(init.model, Model::SenseVoice);
    assert!(read_message::<Init>(&mut b"".as_slice()).unwrap().is_none());
    assert!(read_message::<Init>(&mut b"{}".as_slice()).is_err());
    assert!(
        read_message::<Init>(&mut vec![b' '; usize::try_from(CONTROL_LIMIT).unwrap()].as_slice())
            .is_err()
    );
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"1234").unwrap();
    assert_eq!(read_file(file.path(), 4).unwrap(), b"1234");
    assert!(matches!(read_file(file.path(), 3), Err(Error::Capacity)));
}

#[cfg(unix)]
#[tokio::test]
async fn worker_is_reused_and_cancelled_process_is_reaped() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("worker");
    std::fs::write(
        &program,
        "#!/bin/sh\nwhile IFS= read -r line; do printf 'ok\\n'; done\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let init = Init {
        model: Model::SenseVoice,
        directory: directory.path().to_owned(),
    };
    let files = (directory.path(), directory.path());
    let mut worker = None;
    execute(
        &mut worker,
        &program,
        &init,
        files,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let pid = worker.as_ref().unwrap().child.id();
    execute(
        &mut worker,
        &program,
        &init,
        files,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(worker.as_ref().unwrap().child.id(), pid);
    worker.as_mut().unwrap().stop().await;
    worker = None;
    std::fs::write(
        &program,
        "#!/bin/sh\nread line\nprintf 'ok\\n'\nread line\nread line\n",
    )
    .unwrap();
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        trigger.cancel();
    });
    assert!(matches!(
        execute(&mut worker, &program, &init, files, cancel).await,
        Err(Error::Cancelled)
    ));
    assert!(worker.is_none());
    assert!(Worker::spawn(&directory.path().join("missing")).is_err());
}
