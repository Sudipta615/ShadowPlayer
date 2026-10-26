use crossbeam::channel::{bounded, Receiver};
pub use engine::engine::cue_split::PregapPolicy;
pub use engine::events::{EngineEvent, OutputEvent};
pub use engine::playback_info::{BitPerfectCause, PlaybackInfo, PlaybackState};
pub use engine::playlist::RepeatMode;
pub use engine::{AudioEngine, AudioSource, EngineCommand, EngineConfig, EngineHandle};
use log::{info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

mod device;
mod dsp;
mod transport;

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("Engine initialization failed: {0}")]
    EngineInit(#[from] engine::EngineError),
    #[error("Thread spawn failed: {0}")]
    ThreadSpawn(#[from] std::io::Error),
    #[error("Invalid configuration: {0}")]
    Config(String),
}

pub struct AudioSession {
    handle: EngineHandle,
    running: Arc<AtomicBool>,
    worker_thread: Option<std::thread::JoinHandle<()>>,
    /// The threads that forward engine/output events onto the channels the UI
    /// drains. Kept so `Drop` can join them.
    forwarder_threads: Vec<std::thread::JoinHandle<()>>,
    event_rx: Receiver<EngineEvent>,
    output_event_rx: Receiver<OutputEvent>,
}

/// How long a forwarder waits for the consumer before dropping an event.
const FORWARD_TIMEOUT: Duration = Duration::from_millis(500);

/// Hand an event to the consumer, waiting briefly rather than dropping it.
///
/// The channels used to be drained with `try_send`, so a consumer that was one
/// frame behind silently lost events — including `SourceFinished`, which is
/// what advances the queue to the next track.
fn forward<T>(tx: &crossbeam::channel::Sender<T>, event: T, kind: &str) {
    if let Err(e) = tx.send_timeout(event, FORWARD_TIMEOUT) {
        log::warn!("Dropped a {kind} event: {e}");
    }
}

impl AudioSession {
    pub fn new() -> Result<Self, SessionError> {
        let config = EngineConfig::default();
        Self::with_config(config)
    }

    /// Construct a new hardware audio session with specific configuration.
    pub fn with_config(mut config: EngineConfig) -> Result<Self, SessionError> {
        config.loudness.background_scan_enabled = false;
        let mut engine = AudioEngine::new(config)?;
        if let Err(e) = engine.start() {
            log::warn!("Audio Engine failed to start output hardware: {e}");
        }
        let handle = engine.handle();
        handle.analyzer().set_enabled(false);

        let engine_event_rx = handle.clone_event_receiver();
        let output_event_rx = handle.clone_output_event_receiver();

        let running = Arc::new(AtomicBool::new(true));
        let worker_running = running.clone();

        let worker_thread = std::thread::Builder::new()
            .name("audio-engine-worker".into())
            .spawn(move || {
                info!("Audio Engine worker thread started");
                while worker_running.load(Ordering::Relaxed) {
                    engine.tick_blocking_adaptive();
                }
                info!("Audio Engine worker thread terminating");
            })?;

        // Forward discrete events over crossbeam channels
        let (event_tx, event_rx) = bounded(256);
        let (out_tx, out_rx) = bounded(64);

        let fw_running = running.clone();
        let forwarder = std::thread::Builder::new()
            .name("shadow-engine-events".into())
            .spawn(move || {
                while fw_running.load(Ordering::Relaxed) {
                    let ticker = crossbeam::channel::after(Duration::from_millis(150));
                    crossbeam::channel::select! {
                        recv(engine_event_rx) -> msg => match msg {
                            Ok(ev) => {
                                forward(&event_tx, ev, "engine");
                                while let Ok(ev) = engine_event_rx.try_recv() {
                                    forward(&event_tx, ev, "engine");
                                }
                            }
                            Err(_) => break,
                        },
                        recv(output_event_rx) -> msg => match msg {
                            Ok(out_ev) => {
                                forward(&out_tx, out_ev, "output");
                                while let Ok(out_ev) = output_event_rx.try_recv() {
                                    forward(&out_tx, out_ev, "output");
                                }
                            }
                            Err(_) => break,
                        },
                        recv(ticker) -> _ => {}
                    }
                }
            });

        let forwarder = match forwarder {
            Ok(handle) => handle,
            Err(e) => {
                // The worker owns the engine and would run forever with nothing
                // left able to stop it, so stop it before propagating the
                // failure — `Drop` never runs when a constructor returns `Err`.
                running.store(false, Ordering::Relaxed);
                let _ = worker_thread.join();
                return Err(e.into());
            }
        };

        Ok(Self {
            handle,
            running,
            worker_thread: Some(worker_thread),
            forwarder_threads: vec![forwarder],
            event_rx,
            output_event_rx: out_rx,
        })
    }

    /// Construct a headless audio session backed by a SampleSink (for tests or analysis).
    pub fn with_sink(
        config: EngineConfig,
        sink: Box<dyn engine::SampleSink + Send>,
    ) -> Result<Self, SessionError> {
        let mut engine = AudioEngine::with_sink(config, sink)?;
        let handle = engine.handle();

        let engine_event_rx = handle.clone_event_receiver();

        let running = Arc::new(AtomicBool::new(true));
        let worker_running = running.clone();

        let worker_thread = std::thread::Builder::new()
            .name("shadow-engine-sink-worker".into())
            .spawn(move || {
                while worker_running.load(Ordering::Relaxed) {
                    engine.tick_blocking(Duration::from_millis(5));
                }
            })?;

        let (event_tx, event_rx) = bounded(256);
        let (_out_tx, out_rx) = bounded(64);

        let fw_running = running.clone();
        let forwarder = std::thread::Builder::new()
            .name("shadow-engine-sink-events".into())
            .spawn(move || {
                while fw_running.load(Ordering::Relaxed) {
                    let mut had_event = false;
                    while let Ok(ev) = engine_event_rx.try_recv() {
                        forward(&event_tx, ev, "engine");
                        had_event = true;
                    }
                    if !had_event {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            });

        let forwarder = match forwarder {
            Ok(handle) => handle,
            Err(e) => {
                running.store(false, Ordering::Relaxed);
                let _ = worker_thread.join();
                return Err(e.into());
            }
        };

        Ok(Self {
            handle,
            running,
            worker_thread: Some(worker_thread),
            forwarder_threads: vec![forwarder],
            event_rx,
            output_event_rx: out_rx,
        })
    }

    /// Send a command to the engine thread.
    ///
    /// Returns `false` when the channel is closed: the engine thread is gone and
    /// playback has stopped for good. Every one of these calls used to be
    /// written `let _ = ...`, so a dead engine was indistinguishable from a live
    /// one and a UI toggle could look applied while nothing received it.
    fn send(&self, command: EngineCommand) -> bool {
        match self.handle.send_command(command) {
            Ok(()) => true,
            Err(_) => {
                warn!("The engine command channel is closed; the command was dropped");
                false
            }
        }
    }

    pub fn try_recv_event(&self) -> Option<EngineEvent> {
        self.event_rx.try_recv().ok()
    }

    pub fn try_recv_output_event(&self) -> Option<OutputEvent> {
        self.output_event_rx.try_recv().ok()
    }

    pub fn raw_handle(&self) -> &EngineHandle {
        &self.handle
    }
}

impl Drop for AudioSession {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        // Shut the engine down first so the worker's blocking tick returns.
        self.handle.shutdown();
        // Join the forwarders too: they touch engine receivers, so letting
        // `Drop` return while they are still running is a use-after-free race.
        for thread in self.forwarder_threads.drain(..) {
            let _ = thread.join();
        }
        if let Some(thread) = self.worker_thread.take() {
            let _ = thread.join();
        }
    }
}
