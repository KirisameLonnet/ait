use std::{
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::io::AsyncWriteExt;

use super::catalog::Model;
use crate::Error;

const MAX_ARCHIVE: u64 = 1024 * 1024 * 1024;
const MAX_EXTRACTED: u64 = 3 * MAX_ARCHIVE;

#[derive(Debug)]
enum Status {
    Idle,
    Preparing,
    Ready,
    Failed(Instant),
}

#[derive(Debug)]
pub(super) struct Preparation {
    root: PathBuf,
    model: Model,
    state: Arc<Mutex<Status>>,
    task: Mutex<Option<tokio::task::AbortHandle>>,
}

impl Preparation {
    pub(super) fn new(root: PathBuf, model: Model) -> Self {
        Self {
            root,
            model,
            state: Arc::new(Mutex::new(Status::Idle)),
            task: Mutex::new(None),
        }
    }

    pub(super) fn directory(&self) -> PathBuf {
        self.root.join(self.model.directory())
    }

    pub(super) fn readiness(&self) -> Result<(), Error> {
        let mut state = self.state.lock().map_err(|_| Error::Provider)?;
        match *state {
            Status::Ready => return Ok(()),
            Status::Preparing => return Err(Error::Preparing),
            Status::Failed(at) if at.elapsed() < Duration::from_secs(10) => {
                return Err(Error::ModelDownload);
            }
            Status::Idle | Status::Failed(_) => {}
        }
        if complete(&self.directory(), self.model) {
            *state = Status::Ready;
            return Ok(());
        }
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| Error::Unavailable)?;
        let mut task = self.task.lock().map_err(|_| Error::Provider)?;
        *state = Status::Preparing;
        let (root, model, state) = (self.root.clone(), self.model, self.state.clone());
        *task = Some(
            runtime
                .spawn(async move {
                    tracing::info!(model = model.directory(), "preparing offline speech model");
                    let result = install(&root, model, &model.url()).await;
                    if let Ok(mut state) = state.lock() {
                        *state = if result.is_ok() {
                            Status::Ready
                        } else {
                            Status::Failed(Instant::now())
                        };
                    }
                    if result.is_ok() {
                        tracing::info!(model = model.directory(), "offline speech model ready");
                    } else {
                        tracing::warn!(
                            model = model.directory(),
                            "offline speech model download failed"
                        );
                    }
                })
                .abort_handle(),
        );
        Err(Error::Preparing)
    }
}

impl Drop for Preparation {
    fn drop(&mut self) {
        if let Ok(task) = self.task.get_mut()
            && let Some(task) = task.take()
        {
            task.abort();
        }
    }
}

fn complete(directory: &Path, model: Model) -> bool {
    model.files().iter().all(|file| {
        directory.join(file).metadata().is_ok_and(|meta| {
            if matches!(*file, "espeak-ng-data" | "dict") {
                meta.is_dir()
            } else {
                meta.is_file() && meta.len() > 0
            }
        })
    })
}

async fn install(root: &Path, model: Model, url: &str) -> Result<(), Error> {
    tokio::fs::create_dir_all(root)
        .await
        .map_err(|_| Error::ModelDownload)?;
    let staging = tempfile::tempdir_in(root).map_err(|_| Error::ModelDownload)?;
    let archive_path = staging.path().join("model.tar.bz2");
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|_| Error::ModelDownload)?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| Error::ModelDownload)?
        .error_for_status()
        .map_err(|_| Error::ModelDownload)?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_ARCHIVE)
    {
        return Err(Error::ModelDownload);
    }
    let mut file = tokio::fs::File::create(&archive_path)
        .await
        .map_err(|_| Error::ModelDownload)?;
    let mut size = 0;
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::ModelDownload)? {
        size += chunk.len() as u64;
        if size > MAX_ARCHIVE {
            return Err(Error::ModelDownload);
        }
        file.write_all(&chunk)
            .await
            .map_err(|_| Error::ModelDownload)?;
    }
    file.flush().await.map_err(|_| Error::ModelDownload)?;
    drop(file);
    let root = root.to_owned();
    tokio::task::spawn_blocking(move || {
        let source = std::fs::File::open(archive_path).map_err(|_| Error::ModelDownload)?;
        extract(bzip2::read::BzDecoder::new(source), staging.path(), model)?;
        let extracted = staging.path().join(model.directory());
        if !complete(&extracted, model) {
            return Err(Error::ModelDownload);
        }
        let target = root.join(model.directory());
        if complete(&target, model) {
            return Ok(());
        }
        // Preserve an incomplete existing installation until the replacement is fully validated.
        let previous = staging.path().join("previous");
        if target.exists() {
            std::fs::rename(&target, &previous).map_err(|_| Error::ModelDownload)?;
        }
        if std::fs::rename(&extracted, &target).is_err() {
            if previous.exists() {
                let _ = std::fs::rename(previous, target);
            }
            return Err(Error::ModelDownload);
        }
        Ok(())
    })
    .await
    .map_err(|_| Error::ModelDownload)?
}

fn extract(reader: impl Read, directory: &Path, model: Model) -> Result<(), Error> {
    let mut archive = tar::Archive::new(reader);
    let mut size = 0_u64;
    let mut count = 0;
    for entry in archive.entries().map_err(|_| Error::ModelDownload)? {
        let mut entry = entry.map_err(|_| Error::ModelDownload)?;
        count += 1;
        size = size.checked_add(entry.size()).ok_or(Error::ModelDownload)?;
        let path = entry.path().map_err(|_| Error::ModelDownload)?;
        let valid = path
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
        if !valid
            || !path.starts_with(model.directory())
            || size > MAX_EXTRACTED
            || count > 20_000
            || !(entry.header().entry_type().is_file() || entry.header().entry_type().is_dir())
        {
            return Err(Error::ModelDownload);
        }
        if !entry
            .unpack_in(directory)
            .map_err(|_| Error::ModelDownload)?
        {
            return Err(Error::ModelDownload);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
