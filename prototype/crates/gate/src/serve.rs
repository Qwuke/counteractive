//! Startup and the two listeners (docs 05 sections 3 and 6).
//!
//! `gate.sock` serves inner TLS with this boot's in-memory key, for tor.
//! `issue.sock` serves plain HTTP for the storefront. Both are mode 0660,
//! so the deployment's groups decide who may connect.

use std::fs::{self, DirBuilder, Permissions};
use std::os::unix::fs::{DirBuilderExt as _, FileTypeExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, bail};
use attest::{DevInsecureSource, Role};
use gate_conn::server::{ServeOptions, ServerIdentity, serve_unix};
use tokio::net::UnixListener;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::api;
use crate::attestation::Attestor;
use crate::config::{GateOptions, Release, read_dev_attestation_key};
use crate::models::Models;
use crate::state::Gate;

const GATE_SOCKET: &str = "gate.sock";
const ISSUE_SOCKET: &str = "issue.sock";

/// A started gate. Dropping it stops the listeners without waiting.
pub struct RunningGate {
    shutdown: CancellationToken,
    tasks: TaskTracker,
    gate_socket: PathBuf,
    issue_socket: PathBuf,
}

/// Reads the config, binds both sockets and starts serving, Locked.
pub async fn start(options: GateOptions) -> anyhow::Result<RunningGate> {
    let release = Release::read(&options.release)?;
    let dev_key = read_dev_attestation_key(&options.dev_attest_key)?;
    let models = Arc::new(Models::from_catalogue(&release.config.catalogue, &options.engine)?);
    let identity = ServerIdentity::generate(Role::Gate).context("generating the TLS identity")?;
    let attestor = Attestor::new(identity, DevInsecureSource::new(dev_key, release.sha256));
    let tls = attestor.identity().tls_acceptor();
    let version = release.config.image_version.clone();

    prepare_run_dir(&options.run)?;
    let gate_socket = options.run.join(GATE_SOCKET);
    let issue_socket = options.run.join(ISSUE_SOCKET);
    let gate_listener = bind(&gate_socket)?;
    let issue_listener = bind(&issue_socket)?;

    let shutdown = CancellationToken::new();
    let tasks = TaskTracker::new();
    let gate = Arc::new(Gate::new(release, attestor, models, options.data, shutdown.clone(), tasks.clone()));
    let serve = ServeOptions::default();
    tasks.spawn(serve_unix(gate_listener, Some(tls), api::gate_router(Arc::clone(&gate)), serve, shutdown.clone()));
    tasks.spawn(serve_unix(issue_listener, None, api::issue_router(gate), serve, shutdown.clone()));
    tracing::info!(%version, "gate listening, locked");
    Ok(RunningGate { shutdown, tasks, gate_socket, issue_socket })
}

impl RunningGate {
    /// `gate.sock`.
    pub fn gate_socket(&self) -> &Path {
        &self.gate_socket
    }

    /// `issue.sock`.
    pub fn issue_socket(&self) -> &Path {
        &self.issue_socket
    }

    /// Stops accepting, lets requests in flight finish, stops the timers
    /// and closing tasks, and removes the sockets.
    ///
    /// Live sessions aren't settled: without draining, a stop is like a
    /// crash, and each session's wallet recovers a full refund.
    pub async fn shutdown(self) {
        self.shutdown.cancel();
        self.tasks.close();
        self.tasks.wait().await;
        for socket in [&self.gate_socket, &self.issue_socket] {
            let _ = fs::remove_file(socket);
        }
    }
}

impl Drop for RunningGate {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

/// Creates the run directory with mode 0750, or checks an existing one.
///
/// A socket gets its 0660 mode only after it is bound, so until then the
/// directory alone keeps other users away from `issue.sock`, which mints
/// credit without further authentication. An existing directory must be a
/// real directory that the group can't write and others can't enter. With
/// those bits clear, binding succeeds only for the directory's owner (or
/// root), so no owner check is needed.
fn prepare_run_dir(run: &Path) -> anyhow::Result<()> {
    match DirBuilder::new().mode(0o750).create(run) {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error).with_context(|| format!("creating {}", run.display())),
    }
    let metadata = fs::symlink_metadata(run).with_context(|| format!("checking {}", run.display()))?;
    if !metadata.is_dir() {
        bail!("{} exists and is not a directory", run.display());
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o027 != 0 {
        bail!("{} has mode {mode:o}; the group must not write to it and others must have no access", run.display());
    }
    Ok(())
}

/// Binds a socket with mode 0660, replacing a stale socket file but no
/// other kind of file.
fn bind(path: &Path) -> anyhow::Result<UnixListener> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            fs::remove_file(path).with_context(|| format!("removing stale socket {}", path.display()))?;
        }
        Ok(_) => bail!("{} exists and is not a socket", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("checking {}", path.display())),
    }
    let listener = UnixListener::bind(path).with_context(|| format!("binding {}", path.display()))?;
    fs::set_permissions(path, Permissions::from_mode(0o660))
        .with_context(|| format!("setting permissions on {}", path.display()))?;
    Ok(listener)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_dir_must_keep_others_out() {
        let dir = tempfile::tempdir().unwrap();
        let run = dir.path().join("run");
        prepare_run_dir(&run).unwrap();
        let created = fs::metadata(&run).unwrap().permissions().mode() & 0o777;
        assert_eq!(created & !0o750, 0, "created with mode {created:o}");
        prepare_run_dir(&run).expect("an existing private run dir is accepted");

        for mode in [0o770, 0o755, 0o777] {
            fs::set_permissions(&run, Permissions::from_mode(mode)).unwrap();
            assert!(prepare_run_dir(&run).is_err(), "mode {mode:o} was accepted");
        }

        let file = dir.path().join("file");
        fs::write(&file, b"").unwrap();
        assert!(prepare_run_dir(&file).is_err(), "a plain file was accepted");
    }
}
