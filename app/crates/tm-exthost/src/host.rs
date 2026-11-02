//! Starting and stopping the ext-host process (start-up contract in docs/rust-port/api.md).

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::client::Client;
use crate::Error;

/// How long the JVM may take to print its ready line.
const READY_TIMEOUT: Duration = Duration::from_secs(60);
/// Longest stdout line read while waiting for the ready line.
const MAX_LINE: u64 = 4096;

#[derive(Debug, Clone)]
pub struct HostConfig {
    /// Folder holding `ext-host.jar` and `lib/`.
    pub host_dir: PathBuf,
    /// The TachiMana data directory.
    pub data_dir: PathBuf,
    /// The `java` executable. `None` looks for `<host_dir>/runtime/bin/java`, then `JAVA_HOME`,
    /// then `java` on the PATH.
    pub java: Option<PathBuf>,
}

impl HostConfig {
    pub fn java_path(&self) -> PathBuf {
        if let Some(java) = &self.java {
            return java.clone();
        }
        let exe = if cfg!(windows) { "java.exe" } else { "java" };
        let bundled = self.host_dir.join("runtime").join("bin").join(exe);
        if bundled.is_file() {
            return bundled;
        }
        if let Some(home) = std::env::var_os("JAVA_HOME") {
            let candidate = PathBuf::from(home).join("bin").join(exe);
            if candidate.is_file() {
                return candidate;
            }
        }
        PathBuf::from(exe)
    }

    pub fn jar(&self) -> PathBuf {
        self.host_dir.join("ext-host.jar")
    }
}

/// A running ext-host. Dropping it closes the host's stdin, which makes it exit, and then kills
/// it if it is still there.
pub struct ExtHost {
    child: Child,
    stdin: Option<ChildStdin>,
    port: u16,
    client: Client,
}

impl ExtHost {
    /// Starts the host and waits for its `TM_EXTHOST_READY port=<n>` line.
    pub fn spawn(config: &HostConfig) -> Result<Self, Error> {
        let jar = config.jar();
        if !jar.is_file() {
            return Err(Error::NotInstalled(jar));
        }
        let token = new_token()?;
        let mut child = Command::new(config.java_path())
            .arg("-jar")
            .arg(&jar)
            .arg("--data-dir")
            .arg(&config.data_dir)
            .arg("--port")
            .arg("0")
            .env("TM_EXTHOST_TOKEN", &token)
            .current_dir(&config.host_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Spawn(e.to_string()))?;
        let stdin = child.stdin.take();
        if let Some(stderr) = child.stderr.take() {
            forward_stderr(stderr);
        }
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            return Err(Error::Spawn("no stdout pipe".into()));
        };
        let port = match wait_ready(stdout, READY_TIMEOUT) {
            Ok(port) => port,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let client = Client::new(format!("http://127.0.0.1:{port}"), &token)?;
        tracing::info!(port, "ext-host ready");
        Ok(Self {
            child,
            stdin,
            port,
            client,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Whether the process is still running.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Closes stdin and waits up to `grace` for the host to exit, then kills it.
    pub fn stop(&mut self, grace: Duration) {
        self.stdin.take();
        let deadline = std::time::Instant::now() + grace;
        while std::time::Instant::now() < deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ExtHost {
    fn drop(&mut self) {
        self.stop(Duration::from_secs(2));
    }
}

/// A random 32-byte token, hex encoded.
fn new_token() -> Result<String, Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| Error::Spawn(format!("no randomness: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Parses `TM_EXTHOST_READY port=<n>`; any other line is `None`.
pub fn parse_ready(line: &str) -> Option<u16> {
    let rest = line.trim().strip_prefix("TM_EXTHOST_READY port=")?;
    rest.parse::<u16>().ok().filter(|p| *p != 0)
}

/// Reads stdout on a thread until the ready line, EOF or the timeout.
fn wait_ready<R: Read + Send + 'static>(stdout: R, timeout: Duration) -> Result<u16, Error> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            match (&mut reader).take(MAX_LINE).read_line(&mut line) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(None);
                    return;
                }
                Ok(_) => {
                    if let Some(port) = parse_ready(&line) {
                        let _ = tx.send(Some(port));
                        // Keep draining so the host never blocks on a full pipe.
                        let _ = std::io::copy(&mut reader, &mut std::io::sink());
                        return;
                    }
                    tracing::debug!(line = line.trim_end(), "ext-host stdout");
                }
            }
        }
    });
    match rx.recv_timeout(timeout) {
        Ok(Some(port)) => Ok(port),
        Ok(None) => Err(Error::Spawn(
            "the extension host exited before it was ready (see the log)".into(),
        )),
        Err(_) => Err(Error::Spawn(format!(
            "the extension host was not ready after {} s",
            timeout.as_secs()
        ))),
    }
}

fn forward_stderr<R: Read + Send + 'static>(stderr: R) {
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => tracing::info!(target: "ext-host", "{line}"),
                Err(_) => break,
            }
        }
    });
}

/// Where a packaged app keeps the host: `<resources>/ext-host`.
pub fn default_host_dir(resources: &Path) -> PathBuf {
    resources.join("ext-host")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_line() {
        assert_eq!(parse_ready("TM_EXTHOST_READY port=51234\n"), Some(51234));
        assert_eq!(parse_ready("TM_EXTHOST_READY port=0"), None);
        assert_eq!(parse_ready("TM_EXTHOST_READY port=99999"), None);
        assert_eq!(parse_ready("hello"), None);
        assert_eq!(parse_ready("TM_EXTHOST_READY port=12x"), None);
    }

    #[test]
    fn waits_past_noise_for_the_ready_line() {
        let out = b"Picked up JAVA_TOOL_OPTIONS\nTM_EXTHOST_READY port=4100\nlater\n".to_vec();
        assert_eq!(
            wait_ready(std::io::Cursor::new(out), Duration::from_secs(5)).ok(),
            Some(4100)
        );
    }

    #[test]
    fn eof_before_ready_is_an_error() {
        let out = b"Error: could not find main class\n".to_vec();
        assert!(wait_ready(std::io::Cursor::new(out), Duration::from_secs(5)).is_err());
    }

    #[test]
    fn missing_jar_is_not_installed() {
        let dir = tempfile::tempdir().ok();
        let host_dir = dir
            .as_ref()
            .map(|d| d.path().to_path_buf())
            .unwrap_or_default();
        let cfg = HostConfig {
            host_dir: host_dir.clone(),
            data_dir: host_dir,
            java: None,
        };
        assert!(matches!(ExtHost::spawn(&cfg), Err(Error::NotInstalled(_))));
    }

    #[test]
    fn tokens_are_random_hex() {
        let a = new_token().unwrap_or_default();
        let b = new_token().unwrap_or_default();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
