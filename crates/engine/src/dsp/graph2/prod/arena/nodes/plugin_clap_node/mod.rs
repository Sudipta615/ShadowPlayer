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
use clap_sys::ext::audio_ports::{
    clap_audio_port_info, clap_plugin_audio_ports, CLAP_AUDIO_PORT_IS_MAIN, CLAP_EXT_AUDIO_PORTS,
};
#[cfg(feature = "clap-host")]
use clap_sys::ext::gui::{
    clap_host_gui, clap_plugin_gui, CLAP_EXT_GUI, CLAP_WINDOW_API_COCOA, CLAP_WINDOW_API_WAYLAND,
    CLAP_WINDOW_API_WIN32, CLAP_WINDOW_API_X11,
};
#[cfg(feature = "clap-host")]
use clap_sys::ext::log::{
    clap_host_log, clap_log_severity, CLAP_EXT_LOG, CLAP_LOG_ERROR, CLAP_LOG_FATAL,
    CLAP_LOG_HOST_MISBEHAVING, CLAP_LOG_INFO, CLAP_LOG_PLUGIN_MISBEHAVING, CLAP_LOG_WARNING,
};
#[cfg(feature = "clap-host")]
use clap_sys::ext::params::{
    clap_param_info, clap_plugin_params, CLAP_EXT_PARAMS, CLAP_PARAM_IS_BYPASS, CLAP_PARAM_IS_ENUM,
    CLAP_PARAM_IS_HIDDEN, CLAP_PARAM_IS_PERIODIC, CLAP_PARAM_IS_READONLY, CLAP_PARAM_IS_STEPPED,
};
#[cfg(feature = "clap-host")]
use clap_sys::ext::state::{clap_plugin_state, CLAP_EXT_STATE};
#[cfg(feature = "clap-host")]
use clap_sys::ext::thread_check::{clap_host_thread_check, CLAP_EXT_THREAD_CHECK};
#[cfg(feature = "clap-host")]
use clap_sys::factory::plugin_factory::{clap_plugin_factory, CLAP_PLUGIN_FACTORY_ID};
#[cfg(feature = "clap-host")]
use clap_sys::host::clap_host;
#[cfg(feature = "clap-host")]
use clap_sys::id::CLAP_INVALID_ID;
#[cfg(feature = "clap-host")]
use clap_sys::plugin::{clap_plugin, clap_plugin_descriptor};
#[cfg(feature = "clap-host")]
use clap_sys::process::{
    clap_process, clap_process_status, CLAP_PROCESS_CONTINUE, CLAP_PROCESS_ERROR,
};
#[cfg(feature = "clap-host")]
use clap_sys::stream::{clap_istream, clap_ostream};
#[cfg(feature = "clap-host")]
use clap_sys::string_sizes::{CLAP_NAME_SIZE, CLAP_PATH_SIZE};
#[cfg(feature = "clap-host")]
use clap_sys::version::CLAP_VERSION;

use crate::dsp::clap_rack::{ClapGuiInfo, ClapParamInfo, ClapPluginInfo, ClapRackReport};

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
    gui_ext: *const clap_plugin_gui,
    /// The floating editor has been created (and not yet destroyed) through
    /// `clap.gui`. Some plugins create their window during `create`; others
    /// defer until `show`, which is why this is tracked separately from
    /// [`ClapGuiSignals::open`].
    gui_created: bool,
    gui_api: Option<String>,
    /// The last GUI failure, kept so the rack can explain why the editor
    /// button did nothing.
    gui_error: Option<String>,
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
                if self.gui_created && !self.gui_ext.is_null() {
                    let gui = &*self.gui_ext;
                    if let Some(hide) = gui.hide {
                        hide(self.plugin);
                    }
                    if let Some(destroy) = gui.destroy {
                        destroy(self.plugin);
                    }
                }
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
    /// GUI state shared with the `clap.gui` host callbacks. The plugin may call
    /// `closed` from its own UI thread, so the flag is atomic rather than a
    /// field the callback would need a node pointer to update.
    #[cfg(feature = "clap-host")]
    gui_signals: Arc<ClapGuiSignals>,
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
    gui: Arc<ClapGuiSignals>,
}

/// GUI state the plugin can change from its own UI thread.
#[cfg(feature = "clap-host")]
#[derive(Debug, Default)]
struct ClapGuiSignals {
    open: AtomicBool,
    destroyed: AtomicBool,
    width: AtomicU32,
    height: AtomicU32,
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

/// The `clap.gui` extension table.
///
/// The editor this host opens is always floating: `create(api, true)` gives a
/// plugin its own top-level window, so no parent handle or reparenting is
/// needed and a plugin cannot be embedded into egui by accident. The callbacks
/// below keep the host's view of that window in step with the plugin's and log
/// requests the host cannot fulfil, rather than pretending success.
#[cfg(feature = "clap-host")]
static HOST_GUI_EXTENSION: clap_host_gui = clap_host_gui {
    resize_hints_changed: Some(clap_host_gui_resize_hints_changed),
    request_resize: Some(clap_host_gui_request_resize),
    request_show: Some(clap_host_gui_request_show),
    request_hide: Some(clap_host_gui_request_hide),
    closed: Some(clap_host_gui_closed),
};

#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_gui_resize_hints_changed(_host: *const clap_host) {
    log::debug!("CLAP plugin changed its GUI resize hints");
}

#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_gui_request_resize(
    host: *const clap_host,
    width: u32,
    height: u32,
) -> bool {
    if let Some(context) = host_context(host) {
        context.gui.width.store(width.max(1), Ordering::Relaxed);
        context.gui.height.store(height.max(1), Ordering::Relaxed);
    }
    // A floating editor owns its own window sizing. Remember the request so
    // the rack can show it, but do not claim the host resized anything.
    false
}

#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_gui_request_show(_host: *const clap_host) -> bool {
    // The host has no async hand-off back to the engine thread here. Returning
    // false is the honest answer; the user opens the editor from the rack.
    false
}

#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_gui_request_hide(_host: *const clap_host) -> bool {
    false
}

#[cfg(feature = "clap-host")]
unsafe extern "C" fn clap_host_gui_closed(host: *const clap_host, was_destroyed: bool) {
    if let Some(context) = host_context(host) {
        context.gui.open.store(false, Ordering::Relaxed);
        if was_destroyed {
            context.gui.destroyed.store(true, Ordering::Relaxed);
        }
    }
}

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
    if id == CLAP_EXT_GUI {
        return (&HOST_GUI_EXTENSION as *const clap_host_gui).cast::<c_void>();
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
                        for (destination, source) in plane.iter_mut().zip(f32_planes[index].iter())
                        {
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

/// Read a NUL-terminated string a plugin handed us, tolerating a null pointer
/// and lossy UTF-8 (the C ABI says nothing about encoding).
#[cfg(feature = "clap-host")]
fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned(),
    )
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

// The CLAP host is one type but three jobs: the ABI wrappers and node state
// live here, host construction/loading and state save/load in `host.rs`, and
// parameters/GUI/discovery in `params.rs`. The split keeps the `unsafe` ABI
// surface next to the types it wraps.
mod host;
mod params;

#[cfg(test)]
mod tests;
