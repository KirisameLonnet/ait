use super::*;

fn archive(model: Model, link: bool) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for name in model.files() {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o600);
        header.set_size(if link { 0 } else { 7 });
        if link {
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_link_name("/tmp/outside").unwrap();
        }
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                format!("{}/{name}", model.directory()),
                if link {
                    b"".as_slice()
                } else {
                    b"fixture".as_slice()
                },
            )
            .unwrap();
    }
    builder.into_inner().unwrap()
}

#[test]
fn extraction_validates_files_and_rejects_links() {
    let root = tempfile::tempdir().unwrap();
    let model = Model::SenseVoice;
    assert!(!complete(&root.path().join(model.directory()), model));
    extract(archive(model, false).as_slice(), root.path(), model).unwrap();
    assert!(complete(&root.path().join(model.directory()), model));
    assert!(extract(archive(model, true).as_slice(), root.path(), model).is_err());
    assert!(
        extract(
            archive(model, false).as_slice(),
            root.path(),
            Model::Parakeet
        )
        .is_err()
    );
    let preparation = Preparation::new(root.path().to_owned(), model);
    assert!(preparation.readiness().is_ok());
    std::fs::remove_file(preparation.directory().join("tokens.txt")).unwrap();
    std::fs::create_dir(preparation.directory().join("tokens.txt")).unwrap();
    assert!(!complete(&preparation.directory(), model));
}

#[tokio::test]
async fn download_uses_staging_and_preserves_incomplete_cache_on_failure() {
    use axum::{Router, routing::get};
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let model = Model::SenseVoice;
    let mut compressed = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    compressed.write_all(&archive(model, false)).unwrap();
    let bytes = compressed.finish().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new().route(
        "/model",
        get(move || {
            let bytes = bytes.clone();
            async move { bytes }
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let target = root.path().join(model.directory());
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("partial"), b"keep").unwrap();
    assert!(
        install(root.path(), model, &format!("http://{address}/missing"))
            .await
            .is_err()
    );
    assert!(target.join("partial").exists());
    install(root.path(), model, &format!("http://{address}/model"))
        .await
        .unwrap();
    assert!(complete(&target, model));
    assert!(!target.join("partial").exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    server.abort();
}

#[test]
fn readiness_reports_preparation_and_retryable_failure() {
    let root = tempfile::tempdir().unwrap();
    let preparation = Preparation::new(root.path().to_owned(), Model::SenseVoice);
    *preparation.state.lock().unwrap() = Status::Preparing;
    assert!(matches!(preparation.readiness(), Err(Error::Preparing)));
    *preparation.state.lock().unwrap() = Status::Failed(Instant::now());
    assert!(matches!(preparation.readiness(), Err(Error::ModelDownload)));
}
