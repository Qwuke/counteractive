//! Reaching the instance: commands, files and log tails over SSH, and the
//! tunnel that brings its storefront, fake-stripe and the gate's socket to
//! this machine.
//!
//! A directory on this machine can stand in for the instance, to rehearse
//! without one: commands run there with bash, the services listen on the
//! same loopback ports directly, and a symlink stands in for the forwarded
//! gate socket.

use std::fs::File;
use std::io::Write as _;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::process::workspace_root;
use crate::tree::{Tree, create_private_dir};

/// The instance's directory, under its home: the source, the tree, vLLM
/// and the model.
const INSTANCE_DIR: &str = "ca";
/// How long the tunnel may take to come up.
const TUNNEL_TIMEOUT: Duration = Duration::from_secs(30);
/// How often the tunnel is checked while it comes up.
const TUNNEL_POLL: Duration = Duration::from_millis(100);

/// Where the instance is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum Instance {
    /// A GPU instance, reached over SSH.
    Ssh {
        /// `user@address`, as `ssh` takes it.
        destination: String,
        /// Its `$HOME`, for the forwarded socket's remote path.
        home: String,
    },
    /// A directory on this machine standing in for one, with fake-engine
    /// in place of vLLM: a rehearsal.
    Local {
        /// The directory, as the instance's `~/ca`.
        root: PathBuf,
    },
}

impl Instance {
    /// Where the services run, for people.
    pub fn describe(&self) -> String {
        match self {
            Self::Ssh { destination, .. } => destination.clone(),
            Self::Local { root } => format!("a rehearsal in {}", root.display()),
        }
    }
}

/// The loopback ports the instance's services listen on, which the tunnel
/// forwards to the same ports here, and the console's port that checkout
/// sends a buyer back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ports {
    pub storefront: u16,
    pub fake_stripe: u16,
    pub console: u16,
}

impl Ports {
    /// The storefront, on the instance and here alike.
    pub fn storefront(self) -> SocketAddr {
        (Ipv4Addr::LOCALHOST, self.storefront).into()
    }

    /// fake-stripe, on the instance and here alike.
    pub fn fake_stripe(self) -> SocketAddr {
        (Ipv4Addr::LOCALHOST, self.fake_stripe).into()
    }

    /// Where checkout sends a buyer who paid: the demo console's `/paid`.
    pub fn paid_url(self) -> String {
        format!("http://127.0.0.1:{}/paid", self.console)
    }
}

/// How xtask reaches one instance.
pub struct Transport {
    instance: Instance,
    /// The key `remote init` made.
    key: PathBuf,
    /// The host keys this tree has seen, kept apart from `~/.ssh`.
    known_hosts: PathBuf,
}

impl Transport {
    /// A transport to `instance` with the tree's SSH key.
    pub fn new(tree: &Tree, instance: Instance) -> Self {
        Self { instance, key: ssh_key(tree), known_hosts: tree.root().join("ssh").join("known_hosts") }
    }

    /// The instance this reaches.
    pub fn instance(&self) -> &Instance {
        &self.instance
    }

    /// `ssh` with the tree's key and none of the user's own SSH config.
    fn ssh(&self) -> Command {
        let mut command = Command::new("ssh");
        command.args(["-F", "/dev/null", "-i"]).arg(&self.key).args([
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "ConnectTimeout=20",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "LogLevel=ERROR",
        ]);
        command.arg("-o").arg(format!("UserKnownHostsFile={}", self.known_hosts.display()));
        command
    }

    /// A command running `script` with bash in the instance's directory,
    /// which `$CA` names. A rehearsal also gets `$CA_SRC`, this workspace,
    /// and `$XTASK`, this xtask, so it runs what is already built here.
    fn command(&self, script: &str) -> Command {
        match &self.instance {
            Instance::Ssh { destination, .. } => {
                let full = format!(
                    "mkdir -p \"$HOME/{INSTANCE_DIR}\" && cd \"$HOME/{INSTANCE_DIR}\" && export CA=\"$PWD\" && {script}"
                );
                let mut command = self.ssh();
                command.arg(destination).arg("--").arg(format!("bash -c {}", quote(&full)));
                command
            }
            Instance::Local { root } => {
                let mut command = Command::new("bash");
                command.arg("-c").arg(script).current_dir(root).env("CA", root).env("CA_SRC", workspace_root());
                if let Ok(xtask) = std::env::current_exe() {
                    command.env("XTASK", xtask);
                }
                command
            }
        }
    }

    /// Runs `script` on the instance with its output shown, and fails if
    /// it does.
    pub fn run(&self, script: &str) -> anyhow::Result<()> {
        let status = self.command(script).stdin(Stdio::null()).status().context("running a command on the instance")?;
        ensure!(status.success(), "`{script}` failed on the instance ({status})");
        Ok(())
    }

    /// Runs `script` on the instance and returns what it printed.
    pub fn output(&self, script: &str) -> anyhow::Result<String> {
        let output = self
            .command(script)
            .stdin(Stdio::null())
            .stderr(Stdio::inherit())
            .output()
            .context("running a command on the instance")?;
        ensure!(output.status.success(), "`{script}` failed on the instance ({})", output.status);
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Writes `bytes` to `path` in the instance's directory with `mode`,
    /// replacing the file whole. New directories are private.
    pub fn put(&self, path: &str, bytes: &[u8], mode: u32) -> anyhow::Result<()> {
        let (file, part) = (quote(path), quote(&format!("{path}.part")));
        let script = format!(
            "umask 077 && mkdir -p \"$(dirname {file})\" && cat > {part} && chmod {mode:o} {part} && mv {part} {file}"
        );
        let mut child = self.command(&script).stdin(Stdio::piped()).spawn().context("writing to the instance")?;
        // Dropping the pipe after the write ends `cat`'s input.
        child.stdin.take().context("no pipe to the instance")?.write_all(bytes)?;
        let status = child.wait()?;
        ensure!(status.success(), "writing {path} on the instance failed ({status})");
        Ok(())
    }

    /// Copies this workspace's source to the instance's `src/`, without
    /// build output or any dev tree: those hold keys. A rehearsal runs
    /// from the workspace itself, so this does nothing there.
    pub fn push_source(&self) -> anyhow::Result<()> {
        if let Instance::Local { .. } = self.instance {
            return Ok(());
        }
        let mut tar = Command::new("tar")
            // No macOS metadata, which GNU tar on the instance doesn't know.
            .env("COPYFILE_DISABLE", "1")
            .args(["--no-xattrs", "--no-mac-metadata"])
            .args(["--exclude", "./target", "--exclude", "./.dev", "--exclude", "./.verda", "--exclude", ".DS_Store"])
            .arg("-C")
            .arg(workspace_root())
            .args(["-czf", "-", "."])
            .stdout(Stdio::piped())
            .spawn()
            .context("starting tar")?;
        let archive = tar.stdout.take().context("no pipe from tar")?;
        let unpacked = self
            .command("mkdir -p src && tar -xzf - -C src --no-same-owner --warning=no-unknown-keyword")
            .stdin(archive)
            .status()
            .context("unpacking the source on the instance")?;
        let packed = tar.wait()?;
        ensure!(packed.success() && unpacked.success(), "copying the source failed (tar {packed}, remote {unpacked})");
        Ok(())
    }

    /// Follows `tree/logs/<name>.log` on the instance from its current end,
    /// appending to `out`, until the returned process is killed.
    pub fn tail(&self, name: &str, out: File) -> anyhow::Result<Child> {
        let script = format!("exec tail -n 0 -F {}", quote(&format!("tree/logs/{name}.log")));
        self.command(&script)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("following the instance's {name} log"))
    }

    /// Brings the instance's storefront and fake-stripe to the same
    /// loopback ports here, and its gate socket to the tree's
    /// `run/gate.sock`, for as long as the tunnel lives.
    ///
    /// The gate socket is the operator's way in, for unlocking. The wallet
    /// never uses it: walletd reaches the gate through its onion service.
    pub async fn tunnel(&self, tree: &Tree, ports: Ports) -> anyhow::Result<Tunnel> {
        create_private_dir(&tree.run())?;
        let local = tree.gate_socket();
        remove_if_present(&local)?;
        let (destination, home) = match &self.instance {
            Instance::Ssh { destination, home } => (destination, home),
            Instance::Local { root } => {
                std::os::unix::fs::symlink(root.join("tree/run/gate.sock"), &local)
                    .with_context(|| format!("linking {}", local.display()))?;
                return Ok(Tunnel { ssh: None, socket: local });
            }
        };
        for port in [ports.storefront, ports.fake_stripe] {
            // ssh would only fail to forward, so say who is in the way.
            TcpListener::bind((Ipv4Addr::LOCALHOST, port))
                .with_context(|| format!("127.0.0.1:{port} is taken here; the tunnel forwards it from the instance"))?;
        }
        let mut command = self.ssh();
        command.args(["-N", "-o", "ExitOnForwardFailure=yes", "-o", "StreamLocalBindUnlink=yes"]);
        for port in [ports.storefront, ports.fake_stripe] {
            command.arg("-L").arg(format!("127.0.0.1:{port}:127.0.0.1:{port}"));
        }
        let remote = format!("{home}/{INSTANCE_DIR}/tree/run/gate.sock");
        command.arg("-L").arg(format!("{}:{remote}", local.display())).arg(destination);
        let ssh = command.stdin(Stdio::null()).spawn().context("starting the SSH tunnel")?;
        let mut tunnel = Tunnel { ssh: Some(ssh), socket: local };
        tunnel.wait_up(ports).await?;
        Ok(tunnel)
    }
}

/// The SSH tunnel to the instance, closed on drop.
pub struct Tunnel {
    /// `ssh -N`, or `None` in a rehearsal.
    ssh: Option<Child>,
    /// The local end of the gate socket.
    socket: PathBuf,
}

impl Tunnel {
    async fn wait_up(&mut self, ports: Ports) -> anyhow::Result<()> {
        let deadline = Instant::now() + TUNNEL_TIMEOUT;
        loop {
            if let Some(ssh) = self.ssh.as_mut()
                && let Some(status) = ssh.try_wait()?
            {
                bail!("the SSH tunnel exited ({status}); is the instance up, and the key on it?");
            }
            let forwarded = [ports.storefront(), ports.fake_stripe()]
                .iter()
                .all(|addr| TcpStream::connect_timeout(addr, TUNNEL_POLL).is_ok());
            if forwarded && self.socket.exists() {
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "the SSH tunnel did not come up within {TUNNEL_TIMEOUT:?}");
            tokio::time::sleep(TUNNEL_POLL).await;
        }
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        if let Some(mut ssh) = self.ssh.take() {
            let _ = ssh.kill();
            let _ = ssh.wait();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// The tree's SSH key, which `remote init` makes.
pub fn ssh_key(tree: &Tree) -> PathBuf {
    tree.root().join("ssh").join("id_ed25519")
}

/// `text` as one single-quoted shell word.
pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn remove_if_present(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("removing {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_survives_single_quotes() {
        assert_eq!(quote("plain"), "'plain'");
        assert_eq!(quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn a_rehearsal_runs_in_its_directory_with_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let tree = Tree::new(dir.path().join("tree"));
        let root = dir.path().join("instance");
        std::fs::create_dir(&root).unwrap();
        let transport = Transport::new(&tree, Instance::Local { root: root.clone() });
        transport.put("tree/keys/secret", b"k", 0o600).unwrap();
        let mode = std::fs::metadata(root.join("tree/keys/secret")).unwrap().permissions();
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777, 0o600);
        let printed = transport.output("cat tree/keys/secret; printf ' %s' \"$CA_SRC\"").unwrap();
        assert_eq!(printed, format!("k {}", workspace_root().display()));
        assert!(transport.run("exit 3").is_err());
    }
}
