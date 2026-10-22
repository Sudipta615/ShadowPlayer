//! Loopback control socket for the running instance.
//!
//! The listener is hard-wired to `127.0.0.1` and is advertised with a random
//! token in `<data_dir>/instance.json` (mode 0600 on Unix). It is deliberately
//! **not** a network service: there is no option to bind any other address.
//!
//! The wire format is one JSON object per line. Every request carries the
//! token; a request without it is rejected before it can touch playback.
//! `shadowplayer-remote` and the second-launch activation path are both thin
//! clients of this protocol.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam::channel::{unbounded, Receiver};
use eframe::egui;
use serde::{Deserialize, Serialize};

use super::mpris_service::{MprisCommand, MprisSnapshot};

/// The file the owning instance writes its control endpoint to.
pub const INSTANCE_FILE: &str = "instance.json";
/// Marker written while "Allow multiple instances" is enabled.
///
/// It lives beside the lock rather than in the database because a second
/// launch must decide whether to become a controller *before* it opens the
/// database and audio device.
pub const MULTI_INSTANCE_FILE: &str = "allow-multiple-instances";

/// A running instance's loopback endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlEndpoint {
    pub pid: u32,
    pub port: u16,
    pub token: String,
}

/// One JSON-lines request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlRequest {
    pub token: String,
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

/// One JSON-lines response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<MprisSnapshot>,
}

impl ControlResponse {
    fn ok(state: MprisSnapshot) -> Self {
        Self {
            ok: true,
            error: None,
            state: Some(state),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            state: None,
        }
    }
}

/// Whether the user opted into concurrent instances.
pub fn multi_instance_enabled(data_dir: &Path) -> bool {
    data_dir.join(MULTI_INSTANCE_FILE).exists()
}

/// Persist the multi-instance opt-in where the next launch can read it before
/// opening the database.
pub fn set_multi_instance_enabled(data_dir: &Path, enabled: bool) -> std::io::Result<()> {
    let path = data_dir.join(MULTI_INSTANCE_FILE);
    if enabled {
        std::fs::create_dir_all(data_dir)?;
        std::fs::write(path, b"1\n")?;
    } else if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Read the owning instance's endpoint.
pub fn read_endpoint(data_dir: &Path) -> std::io::Result<ControlEndpoint> {
    let raw = std::fs::read_to_string(data_dir.join(INSTANCE_FILE))?;
    serde_json::from_str(&raw).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid instance endpoint: {e}"),
        )
    })
}

/// Send one command to an instance. Public so the app's second-launch
/// activation path and tests use the exact same client as the remote binary.
pub fn send_control_command(
    endpoint: &ControlEndpoint,
    cmd: &str,
    value: Option<f64>,
) -> std::io::Result<ControlResponse> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = ControlRequest {
        token: endpoint.token.clone(),
        cmd: cmd.to_string(),
        value,
    };
    let mut line = serde_json::to_vec(&request)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response)?;
    serde_json::from_str(&response).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid control response: {e}"),
        )
    })
}

/// Ask the running instance to come to the front.
pub fn activate_existing_instance(data_dir: &Path) -> std::io::Result<()> {
    // The owner may still be wiring its endpoint while a very fast second
    // launch arrives. Retry briefly rather than surfacing a spurious failure.
    let mut last = None;
    for _ in 0..20 {
        match read_endpoint(data_dir) {
            Ok(endpoint) => match send_control_command(&endpoint, "raise", None) {
                Ok(response) if response.ok => return Ok(()),
                Ok(response) => {
                    last = Some(std::io::Error::other(
                        response.error.unwrap_or_else(|| "raise rejected".into()),
                    ))
                }
                Err(e) => last = Some(e),
            },
            Err(e) => last = Some(e),
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("running instance did not answer")))
}

/// The owning instance's control service.
pub struct ControlService {
    commands: Receiver<MprisCommand>,
    snapshot: Arc<Mutex<MprisSnapshot>>,
    repaint: Arc<Mutex<Option<egui::Context>>>,
    shutdown: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
    endpoint_path: PathBuf,
    port: u16,
}

impl ControlService {
    /// Bind a loopback listener and advertise it in `data_dir`.
    ///
    /// The bind address is a constant, not a parameter: this type cannot be
    /// configured to listen on a LAN interface.
    pub fn start(data_dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let token = random_token();
        let endpoint = ControlEndpoint {
            pid: std::process::id(),
            port,
            token: token.clone(),
        };
        write_endpoint(data_dir, &endpoint)?;

        let (tx, commands) = unbounded();
        let snapshot = Arc::new(Mutex::new(MprisSnapshot::default()));
        let repaint = Arc::new(Mutex::new(None));
        let shutdown = Arc::new(AtomicBool::new(false));
        let endpoint_path = data_dir.join(INSTANCE_FILE);

        let join = {
            let snapshot = Arc::clone(&snapshot);
            let repaint = Arc::clone(&repaint);
            let shutdown = Arc::clone(&shutdown);
            std::thread::Builder::new()
                .name("shadowplayer-control".into())
                .spawn(move || {
                    run_listener(listener, token, tx, snapshot, repaint, shutdown);
                })?
        };

        log::info!("Loopback control listening on 127.0.0.1:{port}");
        Ok(Self {
            commands,
            snapshot,
            repaint,
            shutdown,
            join: Some(join),
            endpoint_path,
            port,
        })
    }

    /// Give the UI context to the socket thread so a remote command wakes an
    /// otherwise idle event loop.
    pub fn attach_context(&self, ctx: &egui::Context) {
        if let Ok(mut slot) = self.repaint.lock() {
            if slot.is_none() {
                *slot = Some(ctx.clone());
            }
        }
    }

    /// Publish the state a remote `state` request will see.
    pub fn publish(&self, snapshot: MprisSnapshot) {
        if let Ok(mut slot) = self.snapshot.lock() {
            *slot = snapshot;
        }
    }

    /// Drain transport commands from remote clients.
    pub fn try_recv(&self) -> Option<MprisCommand> {
        self.commands.try_recv().ok()
    }
}

impl Drop for ControlService {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        // Only remove the advertisement if it still names this process. A
        // newer instance may already have replaced it.
        if let Ok(raw) = std::fs::read_to_string(&self.endpoint_path) {
            if let Ok(endpoint) = serde_json::from_str::<ControlEndpoint>(&raw) {
                if endpoint.port == self.port && endpoint.pid == std::process::id() {
                    let _ = std::fs::remove_file(&self.endpoint_path);
                }
            }
        }
    }
}

fn run_listener(
    listener: TcpListener,
    token: String,
    commands: crossbeam::channel::Sender<MprisCommand>,
    snapshot: Arc<Mutex<MprisSnapshot>>,
    repaint: Arc<Mutex<Option<egui::Context>>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                // A malformed or hostile local client must not take the
                // listener thread down.
                if let Err(e) = serve_connection(stream, &token, &commands, &snapshot, &repaint) {
                    log::debug!("Loopback control request failed: {e}");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                log::warn!("Loopback control listener stopped: {e}");
                break;
            }
        }
    }
}

fn serve_connection(
    stream: TcpStream,
    token: &str,
    commands: &crossbeam::channel::Sender<MprisCommand>,
    snapshot: &Arc<Mutex<MprisSnapshot>>,
    repaint: &Arc<Mutex<Option<egui::Context>>>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let request: ControlRequest = match serde_json::from_str(&line) {
        Ok(request) => request,
        Err(e) => {
            write_response(stream, &ControlResponse::error(format!("bad request: {e}")))?;
            return Ok(());
        }
    };
    if !constant_time_eq(request.token.as_bytes(), token.as_bytes()) {
        write_response(stream, &ControlResponse::error("invalid token"))?;
        return Ok(());
    }

    let state = || snapshot.lock().map(|s| s.clone()).unwrap_or_default();
    let response = match request.cmd.as_str() {
        "state" => ControlResponse::ok(state()),
        "play" => enqueue(commands, MprisCommand::Play, state()),
        "pause" => enqueue(commands, MprisCommand::Pause, state()),
        "play_pause" | "toggle" => enqueue(commands, MprisCommand::PlayPause, state()),
        "stop" => enqueue(commands, MprisCommand::Stop, state()),
        "next" => enqueue(commands, MprisCommand::Next, state()),
        "previous" | "prev" => enqueue(commands, MprisCommand::Previous, state()),
        "seek" => {
            let delta = request.value.unwrap_or(0.0);
            enqueue(commands, MprisCommand::SeekRelative(delta), state())
        }
        "seek_to" => {
            let position = request.value.unwrap_or(0.0).max(0.0);
            enqueue(commands, MprisCommand::SetPosition(position), state())
        }
        // `volume` is a percentage-point delta, matching the remote CLI's
        // `volume +5` / `volume -5` spelling.
        "volume" => {
            let delta = (request.value.unwrap_or(0.0) / 100.0) as f32;
            let target = (state().volume + delta).clamp(0.0, 1.0);
            enqueue(commands, MprisCommand::SetVolume(target), state())
        }
        "volume_set" => {
            let target = request.value.unwrap_or(0.0).clamp(0.0, 1.0) as f32;
            enqueue(commands, MprisCommand::SetVolume(target), state())
        }
        "raise" => enqueue(commands, MprisCommand::Raise, state()),
        "quit" => enqueue(commands, MprisCommand::Quit, state()),
        other => ControlResponse::error(format!("unknown command: {other}")),
    };

    if response.ok {
        if let Ok(slot) = repaint.lock() {
            if let Some(ctx) = slot.as_ref() {
                ctx.request_repaint();
            }
        }
    }
    write_response(stream, &response)
}

fn enqueue(
    commands: &crossbeam::channel::Sender<MprisCommand>,
    command: MprisCommand,
    state: MprisSnapshot,
) -> ControlResponse {
    match commands.send(command) {
        Ok(()) => ControlResponse::ok(state),
        Err(_) => ControlResponse::error("control channel closed"),
    }
}

fn write_response(mut stream: TcpStream, response: &ControlResponse) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(response)?;
    line.push(b'\n');
    stream.write_all(&line)
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    if getrandom::fill(&mut bytes).is_err() {
        // Extremely unlikely; a token derived from the clock and pid is still
        // better than refusing to start. The listener is loopback-only.
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (std::process::id(), std::time::SystemTime::now()).hash(&mut hasher);
        let seed = hasher.finish().to_le_bytes();
        for (slot, byte) in bytes.iter_mut().zip(seed.iter().cycle()) {
            *slot = *byte;
        }
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

fn write_endpoint(data_dir: &Path, endpoint: &ControlEndpoint) -> std::io::Result<()> {
    let path = data_dir.join(INSTANCE_FILE);
    let json = serde_json::to_vec(endpoint)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    file.write_all(&json)?;
    file.write_all(b"\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sp_control_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn endpoint_is_loopback_and_commands_cross_the_socket() {
        let dir = temp_dir("socket");
        let service = ControlService::start(&dir).unwrap();
        let endpoint = read_endpoint(&dir).unwrap();
        assert_eq!(endpoint.port, service.port);
        assert!(endpoint.port > 0);

        let state = send_control_command(&endpoint, "state", None).unwrap();
        assert!(state.ok);

        let response = send_control_command(&endpoint, "next", None).unwrap();
        assert!(response.ok);
        assert_eq!(service.try_recv(), Some(MprisCommand::Next));

        let volume = send_control_command(&endpoint, "volume", Some(5.0)).unwrap();
        assert!(volume.ok);
        assert_eq!(service.try_recv(), Some(MprisCommand::SetVolume(0.05)));

        drop(service);
        assert!(
            !dir.join(INSTANCE_FILE).exists(),
            "endpoint removed on drop"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_token_is_rejected_without_touching_transport() {
        let dir = temp_dir("token");
        let service = ControlService::start(&dir).unwrap();
        let mut endpoint = read_endpoint(&dir).unwrap();
        endpoint.token = "not-the-token".to_string();
        let response = send_control_command(&endpoint, "next", None).unwrap();
        assert!(!response.ok);
        assert_eq!(service.try_recv(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn multi_instance_marker_round_trips() {
        let dir = temp_dir("marker");
        assert!(!multi_instance_enabled(&dir));
        set_multi_instance_enabled(&dir, true).unwrap();
        assert!(multi_instance_enabled(&dir));
        set_multi_instance_enabled(&dir, false).unwrap();
        assert!(!multi_instance_enabled(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
