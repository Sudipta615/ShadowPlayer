//! CLAP (CLever Audio Plug-in) Audio Plugin Host Node.
//!
//! Provides hosting and real-time execution of standard CLAP audio effect plugins
//! within the production DSP graph.
//!
//! ## Design & Safety Contract
//! - Control path: library dynamic loading via `libloading`, plugin entry lookup,
//!   factory query, plugin instantiation, configuration (`init`, `activate`).
//! - Realtime audio path (`process_block_f32`):
//!   - Pre-allocated planar channel pointers (`clap_audio_buffer`).
//!   - Zero memory allocations, zero locks, zero blocking operations.
//!   - `unsafe` calls to `clap_plugin.process` follow strict CLAP specification invariants.
//! - Bypass & Fallback: If no plugin is loaded or if processing fails, the node
//!   operates as a bit-exact pass-through.

#[cfg(feature = "clap-host")]
use crate::buffer::MAX_AUDIO_BLOCK_FRAMES;
use crate::dsp::graph2::prod::arena::node::DspNode;
use crate::dsp::pipeline::{DspStageCapability, StageChannelSupport, StagePrecision};

#[cfg(feature = "clap-host")]
use std::cell::Cell;
#[cfg(feature = "clap-host")]
use std::ffi::{c_char, c_void, CStr, CString};
#[cfg(feature = "clap-host")]
use std::path::Path;
#[cfg(feature = "clap-host")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(feature = "clap-host")]
use std::sync::Arc;

#[cfg(feature = "clap-host")]
use clap_sys::audio_buffer::clap_audio_buffer;
#[cfg(feature = "clap-host")]
use clap_sys::entry::clap_plugin_entry;
#[cfg(feature = "clap-host")]
use clap_sys::events::{
    clap_event_header, clap_event_param_value, clap_input_events, clap_output_events,
    CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_PARAM_VALUE,
};
#[cfg(feature = "clap-host")]
use clap_sys::ext::params::{
    clap_param_info, clap_plugin_params, CLAP_EXT_PARAMS, CLAP_PARAM_IS_BYPASS,
    CLAP_PARAM_IS_ENUM, CLAP_PARAM_IS_HIDDEN, CLAP_PARAM_IS_PERIODIC, CLAP_PARAM_IS_READONLY,
    CLAP_PARAM_IS_STEPPED,
};
#[cfg(feature = "clap-host")]
use clap_sys::ext::audio_ports::{
    clap_audio_port_info, clap_plugin_audio_ports, CLAP_AUDIO_PORT_IS_MAIN, CLAP_EXT_AUDIO_PORTS,
};
#[cfg(feature = "clap-host")]
use clap_sys::ext::log::{
    clap_host_log, clap_log_severity, CLAP_EXT_LOG, CLAP_LOG_ERROR, CLAP_LOG_FATAL,
    CLAP_LOG_HOST_MISBEHAVING, CLAP_LOG_INFO, CLAP_LOG_PLUGIN_MISBEHAVING, CLAP_LOG_WARNING,
};
#[cfg(feature = "clap-host")]
use clap_sys::ext::state::{clap_plugin_state, CLAP_EXT_STATE};
#[cfg(feature = "clap-host")]
use clap_sys::ext::thread_check::{clap_host_thread_check, CLAP_EXT_THREAD_CHECK};
#[cfg(feature = "clap-host")]
use clap_sys::id::CLAP_INVALID_ID;
#[cfg(feature = "clap-host")]
use clap_sys::string_sizes::{CLAP_NAME_SIZE, CLAP_PATH_SIZE};
#[cfg(feature = "clap-host")]
use clap_sys::factory::plugin_factory::{clap_plugin_factory, CLAP_PLUGIN_FACTORY_ID};
#[cfg(feature = "clap-host")]
use clap_sys::host::clap_host;
#[cfg(feature = "clap-host")]
use clap_sys::plugin::{clap_plugin, clap_plugin_descriptor};
#[cfg(feature = "clap-host")]
use clap_sys::process::{
    clap_process, clap_process_status, CLAP_PROCESS_CONTINUE, CLAP_PROCESS_ERROR,
};
#[cfg(feature = "clap-host")]
use clap_sys::stream::{clap_istream, clap_ostream};
#[cfg(feature = "clap-host")]
use clap_sys::version::CLAP_VERSION;

use crate::dsp::clap_rack::{ClapParamInfo, ClapPluginInfo, ClapRackReport};

/// A thread-safe handle keeping the dynamically loaded plugin library in memory.
#[cfg(feature = "clap-host")]
struct ClapLibrary {
    _lib: libloading::Library,
    _entry: *const clap_plugin_entry,
}

#[cfg(feature = "clap-host")]
unsafe impl Send for ClapLibrary {}
#[cfg(feature = "clap-host")]
unsafe impl Sync for ClapLibrary {}

#[cfg(feature = "clap-host")]
impl Drop for ClapLibrary {
    fn drop(&mut self) {
        if self._entry.is_null() {
            return;
        }
        // Pair the `entry.init()` the load performed before the library is
        // unloaded: a CLAP entry is initialised once per load, deinitialised
        // once per unload, and a host that skips the second half leaves the
        // library's global state believing a session is still live. The
        // instance that owns this `Arc` is dropped before the library is, so
        // no plugin is alive when this runs.
        unsafe {
            if let Some(deinit) = (*self._entry).deinit {
                deinit();
            }
        }
    }
}

/// An instantiated CLAP plugin instance.
#[cfg(feature = "clap-host")]
struct ClapInstance {
    _library: Arc<ClapLibrary>,
    plugin: *const clap_plugin,
    active: bool,
    /// Whether `start_processing` has been called and not yet undone. CLAP
    /// requires the pair around `activate`/`deactivate`, and a well-behaved
    /// plugin answers `process` with silence until it has been started.
    processing: bool,
    _sample_rate: f64,
    _max_frames: usize,
    /// Channels the plugin's main ports declare; what `process` is told.
    channels: usize,
    /// Whether the plugin paired its main ports for in-place processing. When
    /// it did not, the host must hand it separate output buffers.
    in_place: bool,
    /// Preallocated output staging for a plugin that reserved its own buffers.
    out_scratch: [Vec<f32>; 2],
    params_ext: *const clap_plugin_params,
    state_ext: *const clap_plugin_state,
    pending_param_events: Vec<clap_event_param_value>,
}

#[cfg(feature = "clap-host")]
unsafe impl Send for ClapInstance {}

#[cfg(feature = "clap-host")]
impl Drop for ClapInstance {
    fn drop(&mut self) {
        if !self.plugin.is_null() {
            unsafe {
                let p = &*self.plugin;
                if self.processing {
                    if let Some(stop) = p.stop_processing {
                        stop(self.plugin);
                    }
                }
                if self.active {
                    if let Some(deactivate) = p.deactivate {
                        deactivate(self.plugin);
                    }
                }
                if let Some(destroy) = p.destroy {
                    destroy(self.plugin);
                }
            }
        }
    }
}

#[cfg(feature = "clap-host")]
/// One `param_value` event, as a plugin expects to receive it.
fn param_event(id: u32, value: f64) -> clap_event_param_value {
    clap_event_param_value {
        header: clap_event_header {
            size: std::mem::size_of::<clap_event_param_value>() as u32,
            time: 0,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_: CLAP_EVENT_PARAM_VALUE,
            flags: 0,
        },
        param_id: id,
        cookie: std::ptr::null_mut(),
        note_id: -1,
        port_index: 0,
        channel: -1,
        key: -1,
        value,
    }
}

#[cfg(feature = "clap-host")]
unsafe extern "C" fn in_events_size(list: *const clap_input_events) -> u32 {
    if list.is_null() || (*list).ctx.is_null() {
        return 0;
    }
    let events = (*list).ctx as *const Vec<clap_event_param_value>;
    (*events).len() as u32
}

#[cfg(feature = "clap-host")]
unsafe extern "C" fn in_events_get(
    list: *const clap_input_events,
    index: u32,
) -> *const clap_event_header {
    if list.is_null() || (*list).ctx.is_null() {
        return std::ptr::null();
    }
    let events = &*((*list).ctx as *const Vec<clap_event_param_value>);
    if let Some(ev) = events.get(index as usize) {
        &ev.header as *const _
    } else {
        std::ptr::null()
    }
}

#[cfg(feature = "clap-host")]
struct ClapStreamWriter {
    data: Vec<u8>,
}

#[cfg(feature = "clap-host")]
unsafe extern "C" fn stream_write_fn(
    stream: *const clap_ostream,
    buffer: *const c_void,
    size: u64,
) -> i64 {
    if stream.is_null() || (*stream).ctx.is_null() || buffer.is_null() {
        return -1;
    }
    let writer = &mut *((*stream).ctx as *mut ClapStreamWriter);
    let bytes = std::slice::from_raw_parts(buffer as *const u8, size as usize);
    writer.data.extend_from_slice(bytes);
    size as i64
}

#[cfg(feature = "clap-host")]
struct ClapStreamReader<'a> {
    data: &'a [u8],
    pos: usize,
}

#[cfg(feature = "clap-host")]
unsafe extern "C" fn stream_read_fn(
    stream: *const clap_istream,
    buffer: *mut c_void,
    size: u64,
) -> i64 {
    if stream.is_null() || (*stream).ctx.is_null() || buffer.is_null() {
        return -1;
    }
    let reader = &mut *((*stream).ctx as *mut ClapStreamReader<'_>);
    let available = reader.data.len().saturating_sub(reader.pos);
    let to_read = (size as usize).min(available);
    if to_read > 0 {
        std::ptr::copy_nonoverlapping(
            reader.data[reader.pos..].as_ptr(),
            buffer as *mut u8,
            to_read,
        );
        reader.pos += to_read;
    }
    to_read as i64
}

/// The CLAP Plugin Host Node.
pub struct ClapPluginHostNode {
    sample_rate: f32,
    enabled: bool,
    latency_samples: u32,
    /// The library this instance came from, for display and for re-loading.
    source_path: Option<String>,
    /// How many times the plugin returned `CLAP_PROCESS_ERROR`.
    ///
    /// Nonzero means the plugin failed mid-stream: the node leaves the chain
    /// (see `is_active`) and the rack report carries the reason. Cleared by a
    /// successful load, or by an explicit re-enable (a retry).
    process_errors: AtomicU32,
    #[cfg(feature = "clap-host")]
    instance: Option<ClapInstance>,
    #[cfg(feature = "clap-host")]
    host_descriptor: Box<clap_host>,
    /// The context `host_descriptor.host_data` points at.
    ///
    /// Kept for its address, not for reading: declared after the descriptor so
    /// it outlives every callback a plugin can still make (and prefixed to say
    /// so).
    #[cfg(feature = "clap-host")]
    _host_context: Option<Box<HostContext>>,
    /// f64 → f32 staging for [`DspNode::process_block_f64`]. Allocated here,
    /// never on the audio thread.
    #[cfg(feature = "clap-host")]
    f64_scratch: [Vec<f32>; 2],
}

// Safety: The host node is moved to the audio thread during graph generation swaps.
// The CLAP instance audio processing is exclusively invoked on the realtime audio thread.
unsafe impl Send for ClapPluginHostNode {}

/// State the host extension callbacks reach through `clap_host.host_data`.
///
/// The node owns the box; its address is stable, and the descriptor that
/// points at it outlives every plugin instance the library hands out.
#[cfg(feature = "clap-host")]
struct HostContext {
    /// The thread that created the node — the engine's control thread, which
    /// CLAP designates as the host's main thread.
    main_thread: std::thread::ThreadId,
}

// Whether the current thread is inside one of this host's `process` calls.
//
// The `clap.thread-check` answer has to reflect the thread the callback is
// actually running on, and the engine processes audio on whichever thread
// drives the graph — so the truth is tracked per thread, not stored once.
#[cfg(feature = "clap-host")]
thread_local! {
    static IN_CLAP_PROCESS: Cell<bool> = const { Cell::new(false) };
}

/// Once-per-process latches for the host requests this node cannot honour.
///
/// A plugin may ask every block; the downgrade is worth one log line, not a
/// flood.
#[cfg(feature = "clap-host")]
static RESTART_REQUEST_LOGGED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "clap-host")]
static PROCESS_REQUEST_LOGGED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "clap-host")]
static CALLBACK_REQUEST_LOGGED: AtomicBool = AtomicBool::new(false);

/// The `clap.log` extension table: one function, no per-host state.
#[cfg(feature = "clap-host")]
static HOST_LOG_EXTENSION: clap_host_log = clap_host_log {
    log: Some(clap_host_log_callback),
};

/// The `clap.thread-check` extension table.
#[cfg(feature = "clap-host")]
static HOST_THREAD_CHECK_EXTENSION: clap_host_thread_check = clap_host_thread_check {
    is_main_thread: Some(clap_host_is_main_thread),
    is_audio_thread: Some(clap_host_is_audio_thread),
};

/// The host context a callback was handed, or `None` for a null/foreign host.
#[cfg(feature = "clap-host")]
unsafe fn host_context<'a>(host: *const clap_host) -> Option<&'a HostContext> {
    if host.is_null() {
        return None;
    }
    let data = (*host).host_data;
    if data.is_null() {
        return None;
    }
    Some(&*(data as *const HostContext))
}

/// `clap_host::get_extension`.
///
/// Only extensions this host actually implements are returned: a table of
/// no-op callbacks degrades a plugin as silently as a null answer. A miss is
/// written to the debug log, where a plugin that is limping can be explained
/// without a debugger.
#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_get_extension(
    _host: *const clap_host,
    extension_id: *const c_char,
) -> *const c_void {
    if extension_id.is_null() {
        return std::ptr::null();
    }
    let id = CStr::from_ptr(extension_id);
    if id == CLAP_EXT_LOG {
        return (&HOST_LOG_EXTENSION as *const clap_host_log).cast::<c_void>();
    }
    if id == CLAP_EXT_THREAD_CHECK {
        return (&HOST_THREAD_CHECK_EXTENSION as *const clap_host_thread_check).cast::<c_void>();
    }
    log::debug!("CLAP plugin asked for host extension {id:?}; this host does not implement it");
    std::ptr::null()
}

/// `clap.log`: forward plugin messages to the host's logger.
#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_log_callback(
    _host: *const clap_host,
    severity: clap_log_severity,
    msg: *const c_char,
) {
    // A plugin may log from its audio thread, and the host must not take a
    // logging backend's lock there; a message that arrives mid-process is
    // dropped rather than blocking the callback it came from.
    if IN_CLAP_PROCESS.with(Cell::get) {
        return;
    }
    let text = cstr_to_string(msg).unwrap_or_default();
    match severity {
        CLAP_LOG_FATAL | CLAP_LOG_ERROR => log::error!("CLAP plugin: {text}"),
        CLAP_LOG_WARNING => log::warn!("CLAP plugin: {text}"),
        CLAP_LOG_INFO => log::info!("CLAP plugin: {text}"),
        CLAP_LOG_HOST_MISBEHAVING => {
            log::error!("CLAP plugin reports the host misbehaving: {text}")
        }
        CLAP_LOG_PLUGIN_MISBEHAVING => {
            log::warn!("CLAP plugin reports itself misbehaving: {text}")
        }
        _ => log::debug!("CLAP plugin: {text}"),
    }
}

#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_is_main_thread(host: *const clap_host) -> bool {
    host_context(host).is_some_and(|context| context.main_thread == std::thread::current().id())
}

#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_is_audio_thread(_host: *const clap_host) -> bool {
    IN_CLAP_PROCESS.with(Cell::get)
}

/// `clap_host::request_restart`.
///
/// A live restart means re-creating the plugin's buffers and re-reading its
/// parameter table while the graph holds the node — machinery this host does
/// not have. The request is not swallowed: it is logged once (a plugin may ask
/// every block) so the degradation is visible instead of mysterious.
#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_request_restart(_host: *const clap_host) {
    if !RESTART_REQUEST_LOGGED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "CLAP plugin requested a restart; this host does not restart live plugins — \
             reload the plugin for the change to take effect"
        );
    }
}

/// `clap_host::request_process`.
///
/// The host runs a block-driven graph: every block is processed on schedule,
/// so an immediate-process request needs no action. Logged once, at debug, for
/// the record.
#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_request_process(_host: *const clap_host) {
    if !PROCESS_REQUEST_LOGGED.swap(true, Ordering::Relaxed) {
        log::debug!(
            "CLAP plugin requested an immediate process; this host processes every block on \
             its own schedule"
        );
    }
}

/// `clap_host::request_callback`.
///
/// That callback is the mechanism behind the timer and POSIX-fd extensions,
/// and this host provides neither, so no callback will arrive. Saying so once
/// beats a plugin waiting forever.
#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_request_callback(_host: *const clap_host) {
    if !CALLBACK_REQUEST_LOGGED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "CLAP plugin requested a main-thread callback; this host provides no timer or \
             fd extension, so the callback cannot be delivered"
        );
    }
}

/// What a plugin's own `clap.audio-ports` declaration says about its main bus.
#[cfg(feature = "clap-host")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MainPortDeclaration {
    /// Channel count of the main input port.
    input_channels: u32,
    /// Channel count of the main output port.
    output_channels: u32,
    /// The output port's `in_place_pair` names the input port, so the host may
    /// pass one buffer as both.
    in_place: bool,
}

/// Read the plugin's main audio-port declaration.
///
/// `None` when the plugin does not implement `clap.audio-ports` — the
/// extension is optional, so "the plugin did not say" is a valid answer, and
/// the caller falls back to the graph's own stereo layout. A plugin that *did*
/// say something is held to it: sizing the bus from the declaration is the
/// difference between hosting the plugin and hoping its assumptions match.
#[cfg(feature = "clap-host")]
unsafe fn probe_main_ports(plugin: *const clap_plugin) -> Option<MainPortDeclaration> {
    unsafe {
        if plugin.is_null() {
            return None;
        }
        let get_extension = (*plugin).get_extension?;
        let extension = get_extension(plugin, CLAP_EXT_AUDIO_PORTS.as_ptr());
        if extension.is_null() {
            return None;
        }
        let ports = &*(extension as *const clap_plugin_audio_ports);
        let count = ports.count?;
        let get = ports.get?;

        let main_port = |is_input: bool| -> Option<clap_audio_port_info> {
            let port_count = count(plugin, is_input);
            let mut first = None;
            for index in 0..port_count {
                let mut info = std::mem::zeroed::<clap_audio_port_info>();
                if !get(plugin, index, is_input, &mut info) {
                    continue;
                }
                if info.flags & CLAP_AUDIO_PORT_IS_MAIN != 0 {
                    return Some(info);
                }
                first = first.or(Some(info));
            }
            first
        };

        let input = main_port(true)?;
        let output = main_port(false)?;
        Some(MainPortDeclaration {
            input_channels: input.channel_count,
            output_channels: output.channel_count,
            in_place: output.in_place_pair != CLAP_INVALID_ID && output.in_place_pair == input.id,
        })
    }
}

/// Drive one `clap_plugin.process` call over the node's own planes.
///
/// Both precision entry points funnel through here (`process_block_f64`
/// converts first). The channel count and the in-place permission are the
/// plugin's declaration, read at load: the host never tells a plugin its bus
/// is wider than it is, and only hands it the input buffers for output when
/// the ports are paired.
#[cfg(feature = "clap-host")]
fn run_clap_process(inst: &mut ClapInstance, planes: &mut [&mut [f32]]) -> clap_process_status {
    let frames = planes[0]
        .len()
        .min(planes[1].len())
        .min(MAX_AUDIO_BLOCK_FRAMES);
    let channels = inst.channels.min(planes.len()).min(2);

    let mut in_ptrs = [std::ptr::null_mut::<f32>(); 2];
    for (index, plane) in planes.iter_mut().take(channels).enumerate() {
        in_ptrs[index] = plane.as_mut_ptr();
    }

    // Only an `in_place_pair` lets the host share the buffers; without one the
    // plugin writes into the node's staging and the result is copied back.
    let mut out_ptrs = in_ptrs;
    if !inst.in_place {
        let [stage_left, stage_right] = &mut inst.out_scratch;
        out_ptrs[0] = stage_left.as_mut_ptr();
        out_ptrs[1] = stage_right.as_mut_ptr();
    }

    let in_audio = clap_audio_buffer {
        data32: in_ptrs.as_mut_ptr(),
        data64: std::ptr::null_mut(),
        channel_count: channels as u32,
        latency: 0,
        constant_mask: 0,
    };
    let mut out_audio = clap_audio_buffer {
        data32: out_ptrs.as_mut_ptr(),
        data64: std::ptr::null_mut(),
        channel_count: channels as u32,
        latency: 0,
        constant_mask: 0,
    };

    let has_events = !inst.pending_param_events.is_empty();
    let in_events = if has_events {
        clap_input_events {
            ctx: &inst.pending_param_events as *const _ as *mut c_void,
            size: Some(in_events_size),
            get: Some(in_events_get),
        }
    } else {
        clap_input_events {
            ctx: std::ptr::null_mut(),
            size: None,
            get: None,
        }
    };

    let process_data = clap_process {
        steady_time: -1,
        frames_count: frames as u32,
        transport: std::ptr::null(),
        audio_inputs: &in_audio as *const _,
        audio_outputs: &mut out_audio as *mut _,
        audio_inputs_count: 1,
        audio_outputs_count: 1,
        in_events: if has_events {
            &in_events as *const _
        } else {
            std::ptr::null()
        },
        out_events: std::ptr::null(),
    };

    let mut status = CLAP_PROCESS_CONTINUE;
    unsafe {
        let plugin = &*inst.plugin;
        if let Some(process) = plugin.process {
            // The flag is what makes `clap.thread-check` answer truthfully if
            // the plugin asks while it is being processed.
            IN_CLAP_PROCESS.with(|flag| flag.set(true));
            status = process(inst.plugin, &process_data);
            IN_CLAP_PROCESS.with(|flag| flag.set(false));
        }
    }

    if has_events {
        inst.pending_param_events.clear();
    }

    if !inst.in_place && status != CLAP_PROCESS_ERROR {
        for (index, plane) in planes.iter_mut().take(channels).enumerate() {
            plane[..frames].copy_from_slice(&inst.out_scratch[index][..frames]);
        }
    }

    status
}

#[allow(dead_code)]
impl ClapPluginHostNode {
    /// Creates a new inactive CLAP plugin host node.
    pub fn new(sample_rate: f32) -> Self {
        #[cfg(feature = "clap-host")]
        {
            // The extension callbacks read the node's thread identity through
            // `host_data`; the box's address is what must stay stable.
            let host_context = Box::new(HostContext {
                main_thread: std::thread::current().id(),
            });
            let host_descriptor = Box::new(clap_host {
                clap_version: CLAP_VERSION,
                host_data: Box::as_ref(&host_context) as *const HostContext as *mut c_void,
                name: c"audio-engine".as_ptr(),
                vendor: c"Sudipta615".as_ptr(),
                url: c"https://github.com/Sudipta615/audio_engine".as_ptr(),
                version: c"1.0.0".as_ptr(),
                get_extension: Some(clap_host_get_extension),
                request_restart: Some(clap_host_request_restart),
                request_process: Some(clap_host_request_process),
                request_callback: Some(clap_host_request_callback),
            });

            Self {
                sample_rate,
                enabled: true,
                latency_samples: 0,
                source_path: None,
                process_errors: AtomicU32::new(0),
                instance: None,
                host_descriptor,
                _host_context: Some(host_context),
                f64_scratch: [
                    vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
                    vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
                ],
            }
        }

        #[cfg(not(feature = "clap-host"))]
        {
            Self {
                sample_rate,
                enabled: true,
                latency_samples: 0,
                source_path: None,
                process_errors: AtomicU32::new(0),
            }
        }
    }

    /// Loads and instantiates a CLAP plugin from a dynamic library file path (control thread).
    #[cfg(feature = "clap-host")]
    #[allow(dead_code)]
    pub fn load_plugin(&mut self, path: &Path, plugin_id: Option<&str>) -> Result<(), String> {
        let lib = unsafe {
            libloading::Library::new(path)
                .map_err(|e| format!("Failed to dlopen CLAP library: {e}"))?
        };

        // Standard CLAP entry point: "clap_entry" symbol
        let entry_ptr: *const clap_plugin_entry = unsafe {
            let sym: libloading::Symbol<*const clap_plugin_entry> = lib
                .get(b"clap_entry\0")
                .map_err(|e| format!("Missing clap_entry symbol: {e}"))?;
            *sym
        };

        if entry_ptr.is_null() {
            return Err("clap_entry symbol pointer is null".to_string());
        }

        let entry_ref = unsafe { &*entry_ptr };
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid path string: {e}"))?;

        let init_ok = unsafe {
            if let Some(init) = entry_ref.init {
                init(c_path.as_ptr())
            } else {
                false
            }
        };

        if !init_ok {
            return Err("CLAP entry init() returned false".to_string());
        }

        let factory_ptr = unsafe {
            if let Some(get_factory) = entry_ref.get_factory {
                get_factory(CLAP_PLUGIN_FACTORY_ID.as_ptr())
            } else {
                std::ptr::null()
            }
        };

        if factory_ptr.is_null() {
            return Err("CLAP factory is null".to_string());
        }

        let factory = unsafe { &*(factory_ptr as *const clap_plugin_factory) };
        let count = unsafe {
            if let Some(get_plugin_count) = factory.get_plugin_count {
                get_plugin_count(factory)
            } else {
                0
            }
        };

        if count == 0 {
            return Err("CLAP factory contains no plugins".to_string());
        }

        // Select plugin ID
        let target_id = if let Some(id) = plugin_id {
            CString::new(id).map_err(|e| format!("Invalid plugin ID: {e}"))?
        } else {
            // Pick first plugin
            let desc_ptr = unsafe {
                if let Some(get_plugin_descriptor) = factory.get_plugin_descriptor {
                    get_plugin_descriptor(factory, 0)
                } else {
                    std::ptr::null()
                }
            };
            if desc_ptr.is_null() {
                return Err("Failed to get first CLAP plugin descriptor".to_string());
            }
            let desc = unsafe { &*desc_ptr };
            unsafe { CStr::from_ptr(desc.id).to_owned() }
        };

        let host_ptr: *const clap_host = &*self.host_descriptor;
        let plugin_ptr = unsafe {
            if let Some(create_plugin) = factory.create_plugin {
                create_plugin(factory, host_ptr, target_id.as_ptr())
            } else {
                std::ptr::null()
            }
        };

        if plugin_ptr.is_null() {
            return Err("create_plugin returned null".to_string());
        }

        let plugin = unsafe { &*plugin_ptr };
        let initialized = unsafe {
            if let Some(init) = plugin.init {
                init(plugin_ptr)
            } else {
                false
            }
        };

        if !initialized {
            unsafe {
                if let Some(destroy) = plugin.destroy {
                    destroy(plugin_ptr);
                }
            }
            return Err("Plugin init() returned false".to_string());
        }

        // What the plugin's own port declaration says about the bus it needs.
        // This node feeds exactly two channels and processes in place when the
        // plugin permits it; a wider or asymmetric declaration cannot be
        // honored, and pretending otherwise would hand the plugin pointers it
        // was told not to expect.
        let (channels, in_place) = match unsafe { probe_main_ports(plugin_ptr) } {
            Some(declaration) if declaration.input_channels > 2 || declaration.output_channels > 2 => {
                unsafe {
                    if let Some(destroy) = plugin.destroy {
                        destroy(plugin_ptr);
                    }
                }
                return Err(format!(
                    "the plugin's main ports are {} in / {} out; this host feeds a stereo bus",
                    declaration.input_channels, declaration.output_channels
                ));
            }
            Some(declaration) if declaration.input_channels != declaration.output_channels => {
                unsafe {
                    if let Some(destroy) = plugin.destroy {
                        destroy(plugin_ptr);
                    }
                }
                return Err(format!(
                    "the plugin maps {} input channel(s) to {} output channel(s); this host \
                     processes in place and needs matching ports",
                    declaration.input_channels, declaration.output_channels
                ));
            }
            Some(declaration) if declaration.input_channels == 1 => {
                log::warn!(
                    "CLAP plugin's main ports are mono; only channel 0 is processed and \
                     channel 1 passes through untouched"
                );
                (1usize, declaration.in_place)
            }
            Some(declaration) => (declaration.input_channels as usize, declaration.in_place),
            None => {
                // `clap.audio-ports` is optional; without a declaration the
                // only honest answer left is the stereo, in-place bus the
                // graph provides.
                (2usize, true)
            }
        };

        let activated = unsafe {
            if let Some(activate) = plugin.activate {
                activate(
                    plugin_ptr,
                    self.sample_rate as f64,
                    32,
                    MAX_AUDIO_BLOCK_FRAMES as u32,
                )
            } else {
                false
            }
        };

        if !activated {
            unsafe {
                if let Some(destroy) = plugin.destroy {
                    destroy(plugin_ptr);
                }
            }
            return Err("Plugin activate() returned false".to_string());
        }

        // A conforming plugin stays silent until it is told processing has
        // started, so activating without this would host a mute plugin.
        let processing = unsafe {
            match plugin.start_processing {
                Some(start) => start(plugin_ptr),
                None => true,
            }
        };
        if !processing {
            unsafe {
                if let Some(deactivate) = plugin.deactivate {
                    deactivate(plugin_ptr);
                }
                if let Some(destroy) = plugin.destroy {
                    destroy(plugin_ptr);
                }
            }
            return Err("Plugin start_processing() returned false".to_string());
        }

        let params_ext = unsafe {
            if let Some(get_ext) = plugin.get_extension {
                get_ext(plugin_ptr, CLAP_EXT_PARAMS.as_ptr()) as *const clap_plugin_params
            } else {
                std::ptr::null()
            }
        };

        let state_ext = unsafe {
            if let Some(get_ext) = plugin.get_extension {
                get_ext(plugin_ptr, CLAP_EXT_STATE.as_ptr()) as *const clap_plugin_state
            } else {
                std::ptr::null()
            }
        };

        let lib_arc = Arc::new(ClapLibrary {
            _lib: lib,
            _entry: entry_ptr,
        });

        self.source_path = Some(path.to_string_lossy().to_string());

        // A fresh instance is a fresh start: a failure latch from the plugin
        // that was loaded before it does not carry over.
        self.process_errors.store(0, Ordering::Relaxed);

        self.instance = Some(ClapInstance {
            _library: lib_arc,
            plugin: plugin_ptr,
            active: true,
            processing: true,
            _sample_rate: self.sample_rate as f64,
            _max_frames: MAX_AUDIO_BLOCK_FRAMES,
            channels,
            in_place,
            out_scratch: if in_place {
                [Vec::new(), Vec::new()]
            } else {
                [
                    vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
                    vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
                ]
            },
            params_ext,
            state_ext,
            pending_param_events: Vec::with_capacity(32),
        });

        log::info!(
            "Successfully loaded and activated CLAP plugin: {:?}",
            target_id
        );
        Ok(())
    }

    /// Control path: toggle bypass.
    ///
    /// Re-enabling is also the explicit retry for a plugin that failed
    /// mid-stream: it clears the failure latch so the plugin gets another
    /// chance to process (and latches again if it fails again).
    #[allow(dead_code)]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.process_errors.store(0, Ordering::Relaxed);
        }
    }

    /// Whether a plugin is currently instantiated.
    pub fn is_loaded(&self) -> bool {
        #[cfg(feature = "clap-host")]
        {
            self.instance.is_some()
        }
        #[cfg(not(feature = "clap-host"))]
        {
            false
        }
    }

    /// The dynamic library this node was loaded from.
    pub fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }

    /// The plugin's own descriptor, read from the live instance.
    pub fn describe(&self) -> Option<ClapPluginInfo> {
        #[cfg(feature = "clap-host")]
        {
            let inst = self.instance.as_ref()?;
            if inst.plugin.is_null() {
                return None;
            }
            let desc_ptr = unsafe { (*inst.plugin).desc };
            if desc_ptr.is_null() {
                return None;
            }
            Some(plugin_info_from(unsafe { &*desc_ptr }))
        }
        #[cfg(not(feature = "clap-host"))]
        {
            None
        }
    }

    /// Every parameter the plugin exposes, with the value it reports.
    ///
    /// Must run on a non-audio thread: the CLAP `params` extension reserves
    /// `get_info` and `get_value` for the host's main thread, which is the
    /// engine thread that called `load_plugin`.
    pub fn param_infos(&self) -> Vec<ClapParamInfo> {
        #[cfg(feature = "clap-host")]
        {
            let mut out = Vec::new();
            let Some(inst) = self.instance.as_ref() else {
                return out;
            };
            if inst.params_ext.is_null() || inst.plugin.is_null() {
                return out;
            }
            let ext = unsafe { &*inst.params_ext };
            let Some(count) = ext.count else {
                return out;
            };
            let total = unsafe { count(inst.plugin) };

            for index in 0..total {
                let mut raw = clap_param_info {
                    id: 0,
                    flags: 0,
                    cookie: std::ptr::null_mut(),
                    name: [0; CLAP_NAME_SIZE],
                    module: [0; CLAP_PATH_SIZE],
                    min_value: 0.0,
                    max_value: 0.0,
                    default_value: 0.0,
                };
                let have_info = match ext.get_info {
                    Some(get_info) => unsafe { get_info(inst.plugin, index, &mut raw) },
                    None => false,
                };
                if !have_info {
                    continue;
                }

                let mut value = raw.default_value;
                if let Some(get_value) = ext.get_value {
                    let mut reported = 0.0f64;
                    if unsafe { get_value(inst.plugin, raw.id, &mut reported) } {
                        value = reported;
                    }
                }

                // The plugin renders its own value: for an enumeration that is
                // the name of the mode, which a bare number cannot convey.
                let mut value_text = String::new();
                if let Some(to_text) = ext.value_to_text {
                    let mut buffer = [0 as c_char; CLAP_NAME_SIZE];
                    let wrote = unsafe {
                        to_text(
                            inst.plugin,
                            raw.id,
                            value,
                            buffer.as_mut_ptr(),
                            buffer.len() as u32,
                        )
                    };
                    if wrote {
                        value_text = cstr_buf_to_string(&buffer);
                    }
                }

                out.push(ClapParamInfo {
                    id: raw.id,
                    name: cstr_buf_to_string(&raw.name),
                    module: cstr_buf_to_string(&raw.module),
                    min: raw.min_value,
                    max: raw.max_value,
                    default: raw.default_value,
                    value,
                    value_text,
                    is_stepped: raw.flags & CLAP_PARAM_IS_STEPPED != 0,
                    is_periodic: raw.flags & CLAP_PARAM_IS_PERIODIC != 0,
                    is_readonly: raw.flags & CLAP_PARAM_IS_READONLY != 0,
                    is_hidden: raw.flags & CLAP_PARAM_IS_HIDDEN != 0,
                    is_bypass: raw.flags & CLAP_PARAM_IS_BYPASS != 0,
                    is_enum: raw.flags & CLAP_PARAM_IS_ENUM != 0,
                });
            }
            out
        }
        #[cfg(not(feature = "clap-host"))]
        {
            Vec::new()
        }
    }

    /// The plugin's own text for a value, for enumeration and unit labels.
    pub fn param_text(&self, id: u32, value: f64) -> Option<String> {
        #[cfg(feature = "clap-host")]
        {
            let inst = self.instance.as_ref()?;
            if inst.params_ext.is_null() || inst.plugin.is_null() {
                return None;
            }
            let ext = unsafe { &*inst.params_ext };
            let to_text = ext.value_to_text?;
            let mut buffer = [0 as c_char; CLAP_NAME_SIZE];
            let ok = unsafe {
                to_text(
                    inst.plugin,
                    id,
                    value,
                    buffer.as_mut_ptr(),
                    CLAP_NAME_SIZE as u32,
                )
            };
            if ok {
                return Some(cstr_buf_to_string(&buffer));
            }
            None
        }
        #[cfg(not(feature = "clap-host"))]
        {
            let _ = (id, value);
            None
        }
    }

    /// Whether the plugin implements the `state` extension.
    pub fn has_state(&self) -> bool {
        #[cfg(feature = "clap-host")]
        {
            self.instance
                .as_ref()
                .is_some_and(|inst| !inst.state_ext.is_null())
        }
        #[cfg(not(feature = "clap-host"))]
        {
            false
        }
    }

    /// Everything a host needs to draw the plugin rack.
    ///
    /// A plugin that failed mid-stream is reported here rather than on the
    /// audio thread: the block it failed was already discarded, and this is
    /// where the control path looks for what happened.
    pub fn rack_report(&self) -> ClapRackReport {
        let process_errors = self.process_errors.load(Ordering::Relaxed);
        ClapRackReport {
            plugin: self.describe(),
            params: self.param_infos(),
            path: self.source_path.clone(),
            bypassed: !self.enabled || process_errors > 0,
            has_state: self.has_state(),
            latency_samples: self.latency_samples,
            error: if process_errors > 0 {
                Some(format!(
                    "the plugin returned CLAP_PROCESS_ERROR {process_errors} time{}; it is out \
                     of the chain — reload it (or re-enable) to retry",
                    if process_errors == 1 { "" } else { "s" }
                ))
            } else {
                None
            },
        }
    }

    /// Enumerate the plugins inside a CLAP library without keeping it loaded.
    ///
    /// A host must load the library to ask what it contains, but it need not
    /// instantiate anything: the factory describes every plugin up front. The
    /// library is unloaded again before returning, so probing a folder costs
    /// nothing but the probe.
    #[cfg(feature = "clap-host")]
    pub fn discover(path: &Path) -> Result<Vec<ClapPluginInfo>, String> {
        let lib = unsafe {
            libloading::Library::new(path)
                .map_err(|e| format!("Failed to dlopen CLAP library: {e}"))?
        };

        let entry_ptr: *const clap_plugin_entry = unsafe {
            let sym: libloading::Symbol<*const clap_plugin_entry> = lib
                .get(b"clap_entry\0")
                .map_err(|e| format!("Missing clap_entry symbol: {e}"))?;
            *sym
        };
        if entry_ptr.is_null() {
            return Err("clap_entry symbol pointer is null".to_string());
        }
        let entry = unsafe { &*entry_ptr };

        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("Invalid path string: {e}"))?;
        let init_ok = unsafe {
            match entry.init {
                Some(init) => init(c_path.as_ptr()),
                None => false,
            }
        };
        if !init_ok {
            return Err("CLAP entry init() returned false".to_string());
        }

        // Every exit from here on must run `deinit`, because `init` succeeded.
        let discovered = (|| -> Result<Vec<ClapPluginInfo>, String> {
            let factory_ptr = unsafe {
                match entry.get_factory {
                    Some(get_factory) => get_factory(CLAP_PLUGIN_FACTORY_ID.as_ptr()),
                    None => std::ptr::null(),
                }
            };
            if factory_ptr.is_null() {
                return Err("CLAP factory is null".to_string());
            }
            let factory = unsafe { &*(factory_ptr as *const clap_plugin_factory) };
            let get_count = factory
                .get_plugin_count
                .ok_or_else(|| "Factory has no get_plugin_count".to_string())?;
            let get_descriptor = factory
                .get_plugin_descriptor
                .ok_or_else(|| "Factory has no get_plugin_descriptor".to_string())?;

            let count = unsafe { get_count(factory) };
            let mut found = Vec::with_capacity(count as usize);
            for index in 0..count {
                let desc_ptr = unsafe { get_descriptor(factory, index) };
                if desc_ptr.is_null() {
                    continue;
                }
                found.push(plugin_info_from(unsafe { &*desc_ptr }));
            }
            Ok(found)
        })();

        if let Some(deinit) = entry.deinit {
            unsafe { deinit() };
        }
        discovered
    }

    /// Without the host feature there is no loader, but callers still get a
    /// reason rather than a silent empty list.
    #[cfg(not(feature = "clap-host"))]
    pub fn discover(_path: &std::path::Path) -> Result<Vec<ClapPluginInfo>, String> {
        Err("This build was compiled without CLAP host support".to_string())
    }

    /// Set a parameter value by ID.
    pub fn set_param(&mut self, id: u32, value: f64) {
        #[cfg(feature = "clap-host")]
        {
            if let Some(ref mut inst) = self.instance {
                inst.pending_param_events.push(param_event(id, value));
            }
            // A parameter only reaches a plugin as an event, and a plugin that
            // is not processing never sees one. Handing it over now keeps the
            // plugin's own `get_value` — and so anything a state capture records
            // — in step with the host's cache even while no audio flows.
            self.flush_pending_params();
        }
        #[cfg(not(feature = "clap-host"))]
        {
            let _ = (id, value);
        }
    }

    /// Apply a batch of parameter changes.
    pub fn apply_params(&mut self, batch: &[(u32, f64)]) {
        #[cfg(feature = "clap-host")]
        {
            for &(id, value) in batch {
                if let Some(ref mut inst) = self.instance {
                    inst.pending_param_events.push(param_event(id, value));
                }
            }
            // Once for the batch, not once per parameter.
            self.flush_pending_params();
        }
        #[cfg(not(feature = "clap-host"))]
        {
            let _ = batch;
        }
    }

    /// Hand the plugin every parameter the host has queued, without processing
    /// audio.
    ///
    /// CLAP offers `params.flush` for exactly this: the host calls it while it
    /// is not processing, and the plugin applies the events there and then. The
    /// queued events are deliberately *kept* when the plugin implements no
    /// `flush`, because the next `process` call would otherwise lose them.
    fn flush_pending_params(&mut self) {
        #[cfg(feature = "clap-host")]
        {
            let Some(inst) = self.instance.as_mut() else {
                return;
            };
            if inst.pending_param_events.is_empty()
                || inst.params_ext.is_null()
                || inst.plugin.is_null()
            {
                return;
            }
            let ext = unsafe { &*inst.params_ext };
            let Some(flush) = ext.flush else {
                return;
            };

            let in_events = clap_input_events {
                ctx: &inst.pending_param_events as *const _ as *mut c_void,
                size: Some(in_events_size),
                get: Some(in_events_get),
            };
            // A valid but empty output list: the host takes nothing back from a
            // flush, but a plugin is entitled to write into it and a null
            // pointer would make that undefined.
            let out_events = clap_output_events {
                ctx: std::ptr::null_mut(),
                try_push: None,
            };

            unsafe { flush(inst.plugin, &in_events, &out_events) };
            inst.pending_param_events.clear();
        }
    }

    /// Get current parameter value by ID.
    pub fn get_param_value(&self, _id: u32) -> Option<f64> {
        #[cfg(feature = "clap-host")]
        if let Some(ref inst) = self.instance {
            if !inst.params_ext.is_null() && !inst.plugin.is_null() {
                unsafe {
                    let ext = &*inst.params_ext;
                    if let Some(get_val) = ext.get_value {
                        let mut val = 0.0f64;
                        if get_val(inst.plugin, _id, &mut val) {
                            return Some(val);
                        }
                    }
                }
            }
        }
        None
    }

    /// Return total parameter count reported by the plugin.
    pub fn param_count(&self) -> u32 {
        #[cfg(feature = "clap-host")]
        if let Some(ref inst) = self.instance {
            if !inst.params_ext.is_null() && !inst.plugin.is_null() {
                unsafe {
                    let ext = &*inst.params_ext;
                    if let Some(count) = ext.count {
                        return count(inst.plugin);
                    }
                }
            }
        }
        0
    }

    /// Serialize current plugin state to an opaque byte buffer.
    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        #[cfg(feature = "clap-host")]
        {
            if let Some(ref inst) = self.instance {
                if inst.state_ext.is_null() || inst.plugin.is_null() {
                    return Err("Plugin does not implement state extension".to_string());
                }
                let mut writer = ClapStreamWriter { data: Vec::new() };
                let ostream = clap_ostream {
                    ctx: &mut writer as *mut _ as *mut c_void,
                    write: Some(stream_write_fn),
                };
                unsafe {
                    let ext = &*inst.state_ext;
                    if let Some(save) = ext.save {
                        if save(inst.plugin, &ostream) {
                            return Ok(writer.data);
                        } else {
                            return Err("CLAP state save() returned false".to_string());
                        }
                    }
                }
            }
            Err("No active plugin instance to save state from".to_string())
        }
        #[cfg(not(feature = "clap-host"))]
        {
            Err("clap-host feature disabled".to_string())
        }
    }

    /// Restore plugin state from an opaque byte buffer.
    pub fn load_state(&mut self, _state_bytes: &[u8]) -> Result<(), String> {
        #[cfg(feature = "clap-host")]
        {
            if let Some(ref mut inst) = self.instance {
                if inst.state_ext.is_null() || inst.plugin.is_null() {
                    return Err("Plugin does not implement state extension".to_string());
                }
                let mut reader = ClapStreamReader {
                    data: _state_bytes,
                    pos: 0,
                };
                let istream = clap_istream {
                    ctx: &mut reader as *mut _ as *mut c_void,
                    read: Some(stream_read_fn),
                };
                unsafe {
                    let ext = &*inst.state_ext;
                    if let Some(load) = ext.load {
                        if load(inst.plugin, &istream) {
                            return Ok(());
                        } else {
                            return Err("CLAP state load() returned false".to_string());
                        }
                    }
                }
            }
            Err("No active plugin instance to load state into".to_string())
        }
        #[cfg(not(feature = "clap-host"))]
        {
            Err("clap-host feature disabled".to_string())
        }
    }
}

impl DspNode for ClapPluginHostNode {
    fn capability(&self) -> DspStageCapability {
        DspStageCapability {
            name: "clap_plugin_host",
            channel_support: StageChannelSupport::StereoOnly,
            position: "master insert (post-volume, pre-limiter)",
            stateful: true,
            realtime_safe: true,
            bit_perfect_compatible: false,
            sample_rate_sensitive: true,
            precision: StagePrecision::F32,
        }
    }

    fn is_active(&self) -> bool {
        #[cfg(feature = "clap-host")]
        {
            // A plugin that returned `CLAP_PROCESS_ERROR` is out of the chain:
            // CLAP says the host should stop processing it, and the rack
            // report plays back why. Reloading (or an explicit re-enable)
            // clears the latch.
            self.enabled
                && self.instance.is_some()
                && self.process_errors.load(Ordering::Relaxed) == 0
        }
        #[cfg(not(feature = "clap-host"))]
        {
            false
        }
    }

    fn latency_samples(&self) -> usize {
        self.latency_samples as usize
    }

    fn reset(&mut self) {
        #[cfg(feature = "clap-host")]
        if let Some(ref mut inst) = self.instance {
            if !inst.plugin.is_null() {
                unsafe {
                    let p = &*inst.plugin;
                    if let Some(reset) = p.reset {
                        reset(inst.plugin);
                    }
                }
            }
        }
    }

    fn prepare(&mut self, sample_rate: f32, _max_channels: usize) {
        self.sample_rate = sample_rate;
        #[cfg(feature = "clap-host")]
        if let Some(ref mut inst) = self.instance {
            if !inst.plugin.is_null() && inst.active {
                unsafe {
                    let p = &*inst.plugin;
                    if inst.processing {
                        if let Some(stop) = p.stop_processing {
                            stop(inst.plugin);
                        }
                        inst.processing = false;
                    }
                    if let Some(deactivate) = p.deactivate {
                        deactivate(inst.plugin);
                    }
                    // A plugin with no `activate` cannot be re-configured, but
                    // it is still live: `active` gates the `deactivate` call on
                    // drop, so it must not be cleared for want of one.
                    inst.active = match p.activate {
                        Some(activate) => activate(
                            inst.plugin,
                            sample_rate as f64,
                            32,
                            MAX_AUDIO_BLOCK_FRAMES as u32,
                        ),
                        None => true,
                    };
                    // A rate change is a fresh start, so the pair has to be
                    // re-established or a conforming plugin stays silent.
                    if inst.active {
                        inst.processing = match p.start_processing {
                            Some(start) => start(inst.plugin),
                            None => true,
                        };
                    }
                }
            }
        }
    }

    fn process_block_f32(&mut self, planes: &mut [&mut [f32]]) {
        #[cfg(feature = "clap-host")]
        {
            if !self.is_active() || planes.len() < 2 {
                return;
            }
            let frames = planes[0]
                .len()
                .min(planes[1].len())
                .min(MAX_AUDIO_BLOCK_FRAMES);
            if frames == 0 {
                return;
            }

            let Some(inst) = self.instance.as_mut() else {
                return;
            };
            let status = run_clap_process(inst, planes);
            if status == CLAP_PROCESS_ERROR {
                // The plugin failed mid-block. What is in the buffers is part
                // input and part its half-written output — an in-place plugin
                // leaves no pristine copy to fall back to — so the block is
                // silenced rather than passed through. The failure latches
                // (the node leaves the chain; see `is_active`) and the rack
                // report carries the news to the control thread.
                for plane in planes.iter_mut() {
                    plane[..frames].fill(0.0);
                }
                self.process_errors.fetch_add(1, Ordering::Relaxed);
            }
        }
        #[cfg(not(feature = "clap-host"))]
        {
            let _ = planes;
        }
    }

    fn process_block_f64(&mut self, planes: &mut [&mut [f64]]) {
        #[cfg(feature = "clap-host")]
        {
            if !self.is_active() || planes.len() < 2 {
                return;
            }
            let frames = planes[0]
                .len()
                .min(planes[1].len())
                .min(MAX_AUDIO_BLOCK_FRAMES);
            if frames == 0 {
                return;
            }

            // CLAP is an f32 ABI here (this node does not negotiate the
            // 64-bit port flag), so the block is converted through the
            // preallocated scratch, processed, and converted back. Nothing
            // below allocates: the scratch was sized at construction.
            let status = {
                let Self {
                    instance,
                    f64_scratch,
                    ..
                } = self;
                let Some(inst) = instance.as_mut() else {
                    return;
                };
                let channels = inst.channels.min(planes.len()).min(2);
                let [scratch_left, scratch_right] = f64_scratch;
                let mut f32_planes: [&mut [f32]; 2] =
                    [&mut scratch_left[..frames], &mut scratch_right[..frames]];
                for (index, plane) in planes.iter().take(channels).enumerate() {
                    for (destination, source) in f32_planes[index].iter_mut().zip(plane.iter()) {
                        *destination = *source as f32;
                    }
                }

                let status = run_clap_process(inst, &mut f32_planes);

                // On error the staging holds undefined output; the caller
                // silences the block, so there is nothing worth copying back.
                if status != CLAP_PROCESS_ERROR {
                    for (index, plane) in planes.iter_mut().take(channels).enumerate() {
                        for (destination, source) in plane.iter_mut().zip(f32_planes[index].iter()) {
                            *destination = *source as f64;
                        }
                    }
                }
                status
            };

            if status == CLAP_PROCESS_ERROR {
                for plane in planes.iter_mut() {
                    plane[..frames].fill(0.0);
                }
                self.process_errors.fetch_add(1, Ordering::Relaxed);
            }
        }
        #[cfg(not(feature = "clap-host"))]
        {
            let _ = planes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clap_host_node_lifecycle_without_plugin() {
        let mut node = ClapPluginHostNode::new(48000.0);
        assert!(!node.is_active());
        assert_eq!(node.latency_samples(), 0);

        let mut l = vec![1.0f32; 128];
        let mut r = vec![2.0f32; 128];
        let mut planes = [&mut l[..], &mut r[..]];

        node.process_block_f32(&mut planes);

        // Verification of pass-through behavior
        assert_eq!(l[0], 1.0);
        assert_eq!(r[0], 2.0);
    }

    #[cfg(feature = "clap-host")]
    mod host_extensions {
        use super::*;

        #[test]
        fn the_host_advertises_only_the_extensions_it_implements() {
            let id = |text: &str| CString::new(text).unwrap();
            let log = id("clap.log");
            let thread_check = id("clap.thread-check");
            let params = id("clap.params");
            unsafe {
                assert!(
                    !clap_host_get_extension(std::ptr::null(), log.as_ptr()).is_null(),
                    "clap.log is implemented and must be offered"
                );
                assert!(
                    !clap_host_get_extension(std::ptr::null(), thread_check.as_ptr()).is_null(),
                    "clap.thread-check is implemented and must be offered"
                );
                assert!(
                    clap_host_get_extension(std::ptr::null(), params.as_ptr()).is_null(),
                    "an unimplemented extension must not be faked with an empty table"
                );
                assert!(clap_host_get_extension(std::ptr::null(), std::ptr::null()).is_null());
            }
        }

        #[test]
        fn the_thread_check_answers_about_the_thread_it_is_asked_from() {
            let context = Box::new(HostContext {
                main_thread: std::thread::current().id(),
            });
            let host = clap_host {
                clap_version: CLAP_VERSION,
                host_data: Box::as_ref(&context) as *const HostContext as *mut c_void,
                name: std::ptr::null(),
                vendor: std::ptr::null(),
                url: std::ptr::null(),
                version: std::ptr::null(),
                get_extension: None,
                request_restart: None,
                request_process: None,
                request_callback: None,
            };

            unsafe {
                assert!(clap_host_is_main_thread(&host));
                assert!(!clap_host_is_audio_thread(&host));
                IN_CLAP_PROCESS.with(|flag| flag.set(true));
                assert!(clap_host_is_audio_thread(&host));
                IN_CLAP_PROCESS.with(|flag| flag.set(false));
                assert!(!clap_host_is_audio_thread(&host));

                // A callback that is handed nothing must not guess.
                assert!(!clap_host_is_main_thread(std::ptr::null()));
            }
        }
    }

    /// A synthetic plugin that declares its main audio ports, so the probe can
    /// be exercised without a library on disk.
    #[cfg(feature = "clap-host")]
    mod port_declaration {
        use super::*;

        static CHANNELS: AtomicU32 = AtomicU32::new(2);
        static IN_PLACE: AtomicBool = AtomicBool::new(true);

        unsafe extern "C" fn count(_plugin: *const clap_plugin, _is_input: bool) -> u32 {
            1
        }

        unsafe extern "C" fn get(
            _plugin: *const clap_plugin,
            _index: u32,
            is_input: bool,
            info: *mut clap_audio_port_info,
        ) -> bool {
            if info.is_null() {
                return false;
            }
            let mut declared = std::mem::zeroed::<clap_audio_port_info>();
            declared.id = if is_input { 1 } else { 2 };
            declared.flags = CLAP_AUDIO_PORT_IS_MAIN;
            declared.channel_count = CHANNELS.load(Ordering::Relaxed);
            declared.in_place_pair = if is_input || !IN_PLACE.load(Ordering::Relaxed) {
                CLAP_INVALID_ID
            } else {
                1
            };
            *info = declared;
            true
        }

        static PORTS: clap_plugin_audio_ports = clap_plugin_audio_ports {
            count: Some(count),
            get: Some(get),
        };

        unsafe extern "C" fn get_extension(
            _plugin: *const clap_plugin,
            id: *const c_char,
        ) -> *const c_void {
            if id.is_null() {
                return std::ptr::null();
            }
            if CStr::from_ptr(id) == CLAP_EXT_AUDIO_PORTS {
                return &PORTS as *const clap_plugin_audio_ports as *const c_void;
            }
            std::ptr::null()
        }

        fn plugin() -> clap_plugin {
            let mut plugin = unsafe { std::mem::zeroed::<clap_plugin>() };
            plugin.get_extension = Some(get_extension);
            plugin
        }

        fn declaration(input: u32, output: u32, in_place: bool) -> MainPortDeclaration {
            MainPortDeclaration {
                input_channels: input,
                output_channels: output,
                in_place,
            }
        }

        #[test]
        fn the_main_bus_is_read_from_the_declaration_not_assumed() {
            let plugin = plugin();

            CHANNELS.store(2, Ordering::Relaxed);
            IN_PLACE.store(true, Ordering::Relaxed);
            assert_eq!(
                unsafe { probe_main_ports(&plugin) },
                Some(declaration(2, 2, true))
            );

            CHANNELS.store(1, Ordering::Relaxed);
            assert_eq!(
                unsafe { probe_main_ports(&plugin) },
                Some(declaration(1, 1, true))
            );

            CHANNELS.store(4, Ordering::Relaxed);
            assert_eq!(
                unsafe { probe_main_ports(&plugin) },
                Some(declaration(4, 4, true))
            );

            CHANNELS.store(2, Ordering::Relaxed);
            IN_PLACE.store(false, Ordering::Relaxed);
            assert_eq!(
                unsafe { probe_main_ports(&plugin) },
                Some(declaration(2, 2, false))
            );
        }

        #[test]
        fn a_plugin_without_the_audio_ports_extension_declares_nothing() {
            let mut plugin = unsafe { std::mem::zeroed::<clap_plugin>() };
            plugin.get_extension = None;
            assert_eq!(unsafe { probe_main_ports(&plugin) }, None);
        }
    }
}

/// Read a NUL-terminated string a plugin handed us, tolerating a null pointer
/// and lossy UTF-8 (the C ABI says nothing about encoding).
#[cfg(feature = "clap-host")]
fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned())
}

/// Read one of CLAP's fixed-size inline char arrays.
#[cfg(feature = "clap-host")]
fn cstr_buf_to_string(buffer: &[c_char]) -> String {
    let bytes: Vec<u8> = buffer
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| *c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Copy a plugin descriptor out of plugin-owned memory.
#[cfg(feature = "clap-host")]
fn plugin_info_from(desc: &clap_plugin_descriptor) -> ClapPluginInfo {
    ClapPluginInfo {
        id: cstr_to_string(desc.id).unwrap_or_default(),
        name: cstr_to_string(desc.name).unwrap_or_default(),
        vendor: cstr_to_string(desc.vendor).unwrap_or_default(),
        version: cstr_to_string(desc.version).unwrap_or_default(),
        description: cstr_to_string(desc.description).unwrap_or_default(),
    }
}
