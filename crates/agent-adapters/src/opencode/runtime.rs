//! A fresh authenticated helper belongs to exactly one worker-owned connection.
use std::{path::Path, process::Stdio, time::Duration};

use ait_domain::{DomainError, ErrorCode};
use reqwest::{Method, Url};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::{Child, Command},
};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

use super::{
    failure,
    http::{Api, Version},
};

pub(super) struct Runtime {
    pub(super) api: Api,
    child: Child,
    drain: AbortOnDropHandle<()>,
}

impl Runtime {
    // Keep the child, startup pipes and cleanup in one ownership scope.
    #[allow(clippy::too_many_lines)]
    pub(super) async fn spawn(
        binary: &Path,
        cwd: &Path,
        cancellation: &CancellationToken,
    ) -> Result<Self, DomainError> {
        let version = probe(binary, cwd, cancellation).await?;
        let password = uuid::Uuid::new_v4().simple().to_string();
        let mut command = Command::new(binary);
        command
            .args(["serve", "--hostname", "127.0.0.1", "--port", "0"])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env("OPENCODE_SERVER_USERNAME", "opencode");
        match version {
            Version::V1 => {
                command.env("OPENCODE_SERVER_PASSWORD", &password);
            }
            Version::V2 => {
                command.env("OPENCODE_PASSWORD", &password);
            }
        }
        let mut child = command.spawn().map_err(|_| {
            failure(
                ErrorCode::ProviderFailed,
                "cannot start installed OpenCode executable",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            failure(
                ErrorCode::ProviderFailed,
                "OpenCode stderr pipe unavailable",
            )
        })?;
        let drain = AbortOnDropHandle::new(tokio::spawn(async move {
            let _ = tokio::io::copy(&mut BufReader::new(stderr), &mut tokio::io::sink()).await;
        }));
        let stdout = child.stdout.take().ok_or_else(|| {
            failure(
                ErrorCode::ProviderFailed,
                "OpenCode stdout pipe unavailable",
            )
        })?;
        let mut stdout = BufReader::new(stdout.take(32_768));
        let startup = async {
            loop {
                let mut line = String::new();
                if stdout.read_line(&mut line).await.map_err(|_| {
                    failure(
                        ErrorCode::ProviderFailed,
                        "OpenCode startup output interrupted",
                    )
                })? == 0
                {
                    return Err(failure(
                        ErrorCode::ProviderFailed,
                        "OpenCode exited before readiness",
                    ));
                }
                if let Some((_, address)) = line.split_once("server listening on ") {
                    return Url::parse(address.trim()).map_err(|_| {
                        failure(
                            ErrorCode::ProviderFailed,
                            "OpenCode returned invalid server address",
                        )
                    });
                }
            }
        };
        let result = tokio::select! {
            () = cancellation.cancelled() => Err(failure(ErrorCode::RunCancelled, "OpenCode startup cancelled")),
            result = tokio::time::timeout(Duration::from_secs(30), startup) =>
                result.unwrap_or_else(|_| Err(failure(ErrorCode::ProviderFailed, "OpenCode startup timed out"))),
        };
        let api = match result
            .and_then(|url| Api::new(version, url, password, cwd.to_string_lossy().into_owned()))
        {
            Ok(api) => api,
            Err(error) => {
                let _ = child.kill().await;
                return Err(error);
            }
        };
        // Drain stdout after readiness as well; neither provider pipe can block execution.
        let stdout = stdout.into_inner().into_inner();
        let drain_stdout = AbortOnDropHandle::new(tokio::spawn(async move {
            let _ = tokio::io::copy(&mut BufReader::new(stdout), &mut tokio::io::sink()).await;
        }));
        let drain = AbortOnDropHandle::new(tokio::spawn(async move {
            let _ = tokio::join!(drain, drain_stdout);
        }));
        let mut runtime = Self { api, child, drain };
        let path = match version {
            Version::V1 => "/global/health",
            Version::V2 => "/api/info",
        };
        let readiness = tokio::select! {
            () = cancellation.cancelled() => Err(failure(ErrorCode::RunCancelled, "OpenCode startup cancelled")),
            result = runtime.api.json(Method::GET, path, None) => result,
        };
        if let Err(error) = readiness {
            runtime.close().await;
            return Err(error);
        }
        Ok(runtime)
    }

    pub(super) async fn close(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        self.drain.abort();
    }
}

async fn probe(
    binary: &Path,
    cwd: &Path,
    cancel: &CancellationToken,
) -> Result<Version, DomainError> {
    let mut child = Command::new(binary)
        .arg("--version")
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| failure(ErrorCode::ProviderFailed, "OpenCode executable unavailable"))?;
    let mut output = child
        .stdout
        .take()
        .ok_or_else(|| {
            failure(
                ErrorCode::ProviderFailed,
                "OpenCode version pipe unavailable",
            )
        })?
        .take(2049);
    let read = async {
        let mut bytes = Vec::new();
        output.read_to_end(&mut bytes).await.map_err(|_| {
            failure(
                ErrorCode::ProviderFailed,
                "OpenCode version probe interrupted",
            )
        })?;
        if bytes.len() > 2048 {
            return Err(failure(
                ErrorCode::ProviderFailed,
                "OpenCode version output exceeded limit",
            ));
        }
        let status = child
            .wait()
            .await
            .map_err(|_| failure(ErrorCode::ProviderFailed, "OpenCode version probe failed"))?;
        if !status.success() {
            return Err(failure(
                ErrorCode::ProviderFailed,
                "OpenCode version probe failed",
            ));
        }
        Version::parse(std::str::from_utf8(&bytes).map_err(|_| {
            failure(
                ErrorCode::ProviderFailed,
                "invalid OpenCode version encoding",
            )
        })?)
    };
    let result = tokio::select! {
        () = cancel.cancelled() => Err(failure(ErrorCode::RunCancelled, "OpenCode version probe cancelled")),
        result = tokio::time::timeout(Duration::from_secs(5), read) => result
            .unwrap_or_else(|_| Err(failure(ErrorCode::ProviderFailed, "OpenCode version probe timed out"))),
    };
    if result.is_err() {
        let _ = child.kill().await;
    }
    result
}
