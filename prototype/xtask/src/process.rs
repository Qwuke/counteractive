//! Building and running the prototype's binaries as separate processes.
//!
//! Each service logs (stdout and stderr) to its own file. `up` detaches
//! them and records their pids for `down`; `e2e` keeps them and kills
//! whatever is left when it ends, even on an error.

use std::fs::OpenOptions;
use std::net::SocketAddr;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};

/// How long a service may take to start listening.
pub const START_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a service may take to exit after SIGTERM before it is killed.
const STOP_TIMEOUT: Duration = Duration::from_secs(15);
/// How often readiness and exit are polled.
const POLL: Duration = Duration::from_millis(50);

/// The service binaries built together; walletd builds on its own.
pub const SERVICES: [&str; 5] = ["gate", "storefront", "fake-engine", "fake-stripe", "wallet-cli"];
/// The features that compile the crash points into the services built
/// together, and into walletd.
pub const SERVICE_CRASH_POINTS: [&str; 2] = ["gate/crash-points", "wallet-cli/crash-points"];
pub const WALLETD_CRASH_POINTS: [&str; 1] = ["walletd/crash-points"];
/// Where, inside xtask's own target directory, the crash-point builds go.
const CRASH_POINT_TARGET: &str = "crash-points";

/// Where the built binaries are, and which program, if any, a run starts
/// with a crash point armed.
#[derive(Debug, Clone)]
pub struct Binaries {
    dir: PathBuf,
    /// A program and the `crash_point!` it aborts at (kill tests only).
    armed: Option<(&'static str, &'static str)>,
}

impl Binaries {
    /// Builds every service binary with the profile xtask itself was built
    /// with, so they land next to it. `tor` adds walletd's Arti dialer.
    ///
    /// walletd builds on its own: cargo unifies features across the
    /// packages of one build, which would give walletd's `gate-conn` the
    /// TCP dialer `wallet-cli` enables (ARCHITECTURE 8.2 rule 5).
    pub fn build(tor: bool) -> anyhow::Result<Self> {
        cargo_build(&SERVICES, &[], None)?;
        cargo_build(&["walletd"], if tor { &["walletd/arti"] } else { &[] }, None)?;
        Ok(Self { dir: xtask_dir()?, armed: None })
    }

    /// Builds the same binaries with every `crash_point!` compiled in
    /// (`gate`, `walletd` and `wallet-cli` with their `crash-points`
    /// feature), for the kill tests.
    ///
    /// They go to a target directory of their own, `crash-points/` inside
    /// xtask's, so they never replace the binaries the other steps, `up`
    /// and users run: those must carry no crash point.
    pub fn build_with_crash_points() -> anyhow::Result<Self> {
        let own = xtask_dir()?;
        let (Some(target), Some(profile)) = (own.parent(), own.file_name()) else {
            bail!("xtask's directory {} is not inside a cargo target directory", own.display());
        };
        let target = target.join(CRASH_POINT_TARGET);
        cargo_build(&SERVICES, &SERVICE_CRASH_POINTS, Some(&target))?;
        cargo_build(&["walletd"], &WALLETD_CRASH_POINTS, Some(&target))?;
        Ok(Self { dir: target.join(profile), armed: None })
    }

    /// The same binaries, with `program` started with `point` armed: it
    /// aborts when it reaches that `crash_point!`. Only a crash-point build
    /// has the point; any other binary ignores it.
    pub fn arming(&self, program: &'static str, point: &'static str) -> Self {
        Self { dir: self.dir.clone(), armed: Some((program, point)) }
    }

    /// A binary's path.
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// A command running the binary `name`, with its crash point armed if
    /// this is the armed program, and with none armed otherwise, whatever
    /// xtask's own environment holds.
    pub fn command(&self, name: &str) -> Command {
        let mut command = Command::new(self.path(name));
        match self.armed {
            Some((program, point)) if program == name => command.env(crash_point::ENV_VAR, point),
            _ => command.env_remove(crash_point::ENV_VAR),
        };
        command
    }
}

/// The directory xtask's own binary is in, where `cargo build` with the
/// same profile and target directory puts the others.
fn xtask_dir() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("finding xtask's own path")?;
    Ok(exe.parent().context("xtask has no parent directory")?.to_owned())
}

/// Runs `cargo build` for `packages` with `features`, into `target_dir`
/// when given (otherwise cargo's own choice, the one xtask was built in).
fn cargo_build(packages: &[&str], features: &[&str], target_dir: Option<&Path>) -> anyhow::Result<()> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.arg("build").arg("--manifest-path").arg(workspace_root().join("Cargo.toml"));
    for package in packages {
        command.args(["-p", package]);
    }
    if !features.is_empty() {
        command.args(["--features", &features.join(",")]);
    }
    if let Some(dir) = target_dir {
        command.arg("--target-dir").arg(dir);
    }
    if !cfg!(debug_assertions) {
        command.arg("--release");
    }
    let status = command.status().context("running cargo build")?;
    if !status.success() {
        bail!("cargo build -p {} failed", packages.join(" -p "));
    }
    Ok(())
}

/// The prototype workspace root.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask lives inside the workspace").to_owned()
}

/// A running service.
pub struct Service {
    name: &'static str,
    child: Child,
    log: PathBuf,
    /// Where this run's lines start in the log, which earlier runs share.
    log_start: u64,
    detached: bool,
}

impl Service {
    /// Starts the built binary `name` with `args`, logging to
    /// `<logs>/<name>.log`.
    pub fn spawn(
        binaries: &Binaries,
        name: &'static str,
        args: &[&std::ffi::OsStr],
        logs: &Path,
    ) -> anyhow::Result<Self> {
        Self::start(binaries.command(name), name, args, logs)
    }

    /// Starts `program` as `name`, logging to `<logs>/<name>.log`.
    pub fn spawn_program(
        program: &Path,
        name: &'static str,
        args: &[&std::ffi::OsStr],
        logs: &Path,
    ) -> anyhow::Result<Self> {
        Self::start(Command::new(program), name, args, logs)
    }

    /// Runs `command` as `name` in its own process group, so a Ctrl-C meant
    /// for xtask doesn't reach it, appending its output to its log.
    fn start(mut command: Command, name: &'static str, args: &[&std::ffi::OsStr], logs: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(logs).with_context(|| format!("creating {}", logs.display()))?;
        let log = logs.join(format!("{name}.log"));
        let out = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .with_context(|| format!("opening {}", log.display()))?;
        let log_start = out.metadata()?.len();
        let child = command
            .args(args)
            .stdin(Stdio::null())
            .stdout(out.try_clone()?)
            .stderr(out)
            .process_group(0)
            .spawn()
            .with_context(|| format!("starting {name}"))?;
        Ok(Self { name, child, log, log_start, detached: false })
    }

    /// The service's name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Its process id.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Its log file.
    pub fn log(&self) -> &Path {
        &self.log
    }

    /// Waits for the `listening on <addr>` line a TCP service prints once
    /// bound, and returns the address. Only lines written after `skip`
    /// bytes of the log count, so a restart doesn't read an old line.
    pub async fn listening(&mut self, skip: u64, timeout: Duration) -> anyhow::Result<SocketAddr> {
        let deadline = Instant::now() + timeout;
        loop {
            let text = std::fs::read(&self.log).unwrap_or_default();
            let fresh =
                String::from_utf8_lossy(text.get(usize::try_from(skip).unwrap_or(usize::MAX)..).unwrap_or_default())
                    .into_owned();
            if let Some(addr) = fresh.lines().find_map(|line| line.strip_prefix("listening on ")) {
                return addr.trim().parse().with_context(|| format!("{} printed a bad address", self.name));
            }
            self.wait_step(deadline)?;
            tokio::time::sleep(POLL).await;
        }
    }

    /// Waits until the service has written `path`.
    pub async fn file_written(&mut self, path: &Path) -> anyhow::Result<()> {
        let deadline = Instant::now() + START_TIMEOUT;
        while !path.is_file() {
            self.wait_step(deadline)?;
            tokio::time::sleep(POLL).await;
        }
        Ok(())
    }

    /// Waits until every socket accepts a connection.
    pub async fn accepting(&mut self, sockets: &[PathBuf]) -> anyhow::Result<()> {
        let deadline = Instant::now() + START_TIMEOUT;
        while !sockets.iter().all(|socket| std::os::unix::net::UnixStream::connect(socket).is_ok()) {
            self.wait_step(deadline)?;
            tokio::time::sleep(POLL).await;
        }
        Ok(())
    }

    fn wait_step(&mut self, deadline: Instant) -> anyhow::Result<()> {
        if let Some(status) = self.child.try_wait()? {
            bail!("{} exited ({status}) while starting; see {}", self.name, self.log.display());
        }
        if Instant::now() > deadline {
            bail!("{} did not start in time; see {}", self.name, self.log.display());
        }
        Ok(())
    }

    /// SIGTERM, then SIGKILL if it hasn't exited in time.
    pub async fn stop(mut self) -> anyhow::Result<()> {
        terminate(self.pid())?;
        let deadline = Instant::now() + STOP_TIMEOUT;
        while self.child.try_wait()?.is_none() {
            if Instant::now() > deadline {
                tracing::warn!(service = self.name, "did not stop after SIGTERM; killing it");
                self.child.kill()?;
                self.child.wait()?;
                break;
            }
            tokio::time::sleep(POLL).await;
        }
        Ok(())
    }

    /// SIGKILL, as a crash, and reaps it.
    pub fn kill(mut self) -> anyhow::Result<()> {
        self.child.kill()?;
        self.child.wait()?;
        Ok(())
    }

    /// Waits up to `timeout` for the service to exit by itself, as an armed
    /// crash point makes it, and reaps it. Returns how it ended and what
    /// this run logged.
    pub async fn exited(mut self, timeout: Duration) -> anyhow::Result<(ExitStatus, String)> {
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status;
            }
            if Instant::now() > deadline {
                bail!("{} still runs {timeout:?} later; see {}", self.name, self.log.display());
            }
            tokio::time::sleep(POLL).await;
        };
        let log = std::fs::read(&self.log).with_context(|| format!("reading {}", self.log.display()))?;
        let own = log.get(usize::try_from(self.log_start)?..).unwrap_or_default();
        Ok((status, String::from_utf8_lossy(own).into_owned()))
    }

    /// Leaves the service running after xtask exits. Returns its pid.
    pub fn detach(mut self) -> u32 {
        self.detached = true;
        self.pid()
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if !self.detached && matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Sends SIGTERM to a process.
pub fn terminate(pid: u32) -> anyhow::Result<()> {
    signal(pid, "-TERM")
}

/// Sends SIGKILL to a process.
pub fn kill(pid: u32) -> anyhow::Result<()> {
    signal(pid, "-KILL")
}

/// Whether a process with this pid exists and runs `name`, so `down`
/// never signals an unrelated process that reused a recorded pid.
pub fn is_running(pid: u32, name: &str) -> bool {
    let Ok(output) = Command::new("ps").args(["-p", &pid.to_string(), "-o", "comm="]).output() else {
        return false;
    };
    let command = String::from_utf8_lossy(&output.stdout);
    output.status.success() && Path::new(command.trim()).file_name().is_some_and(|file| file == name)
}

fn signal(pid: u32, signal: &str) -> anyhow::Result<()> {
    let status = Command::new("kill").args([signal, &pid.to_string()]).status().context("running kill")?;
    if !status.success() {
        bail!("kill {signal} {pid} failed");
    }
    Ok(())
}
