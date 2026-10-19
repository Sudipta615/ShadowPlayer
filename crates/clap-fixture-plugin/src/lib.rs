//! A minimal but real CLAP plugin, used as a fixture for the host.
//!
//! The player's plugin rack can only be believed if something actually loads
//! through it. A CLAP plugin is a dynamic library speaking a C ABI, so a
//! fixture cannot be a mock compiled into the test: it has to be a `.clap`
//! file that the host opens with `dlopen`. This crate is that file.
//!
//! It is deliberately dull DSP — a gain, a dry/wet blend, a trim — because what
//! is under test is the *host*: enumeration, parameter plumbing, bypass, and
//! state capture. Two plugins live in the one library so a host that has to
//! choose between the plugins in a library has something to choose between.
//!
//! Everything here is `unsafe extern "C"` by necessity: the signatures are the
//! plugin API's, and the pointers come from the host.

use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicU32, Ordering};

use clap_sys::entry::clap_plugin_entry;
use clap_sys::events::{
    clap_event_param_value, clap_input_events, clap_output_events, CLAP_CORE_EVENT_SPACE_ID,
    CLAP_EVENT_PARAM_VALUE,
};
use clap_sys::ext::audio_ports::{
    clap_audio_port_info, clap_plugin_audio_ports, CLAP_AUDIO_PORT_IS_MAIN, CLAP_EXT_AUDIO_PORTS,
    CLAP_PORT_STEREO,
};
use clap_sys::ext::params::{
    clap_param_info, clap_plugin_params, CLAP_EXT_PARAMS, CLAP_PARAM_IS_AUTOMATABLE,
    CLAP_PARAM_IS_BYPASS, CLAP_PARAM_IS_ENUM, CLAP_PARAM_IS_STEPPED,
};
use clap_sys::ext::state::{clap_plugin_state, CLAP_EXT_STATE};
use clap_sys::factory::plugin_factory::{clap_plugin_factory, CLAP_PLUGIN_FACTORY_ID};
use clap_sys::host::clap_host;
use clap_sys::id::CLAP_INVALID_ID;
use clap_sys::plugin::{clap_plugin, clap_plugin_descriptor};
use clap_sys::process::{
    clap_process, clap_process_status, CLAP_PROCESS_CONTINUE, CLAP_PROCESS_ERROR,
};
use clap_sys::stream::{clap_istream, clap_ostream};
use clap_sys::version::CLAP_VERSION;

// ── Shared FFI tables ───────────────────────────────────────────────────────
//
// CLAP's structs are function-pointer tables. Most are per-instance and the
// bindings decline to mark them `Sync` for that reason; the ones below are
// immutable statics, so the assertion is ours to make. `repr(transparent)`
// keeps the layout identical to the wrapped struct, which is what lets
// `clap_entry` be exported through the wrapper.

#[repr(transparent)]
pub struct Shared<T>(T);

// Safety: every use of this wrapper is for a `static` whose contents never
// change after startup. Nothing in CLAP mutates a factory, extension or entry
// table.
unsafe impl<T> Sync for Shared<T> {}

/// A NUL-terminated byte string a descriptor or table can point at.
macro_rules! cstr {
    ($s:literal) => {
        concat!($s, "\0").as_bytes()
    };
}

const GAIN_ID: &[u8] = cstr!("com.shadowplayer.fixture.gain");
const TRIM_ID: &[u8] = cstr!("com.shadowplayer.fixture.trim");
const FAULTY_ID: &[u8] = cstr!("com.shadowplayer.fixture.faulty");
const VENDOR: &[u8] = cstr!("ShadowPlayer");
const URL: &[u8] = cstr!("https://example.invalid/shadowplayer");
const FIXTURE_VERSION: &[u8] = cstr!("1.0.0");

const FEATURE_AUDIO_EFFECT: &[u8] = cstr!("audio-effect");
const FEATURE_STEREO: &[u8] = cstr!("stereo");

#[repr(transparent)]
struct FeatureList([*const c_char; 3]);

// Safety: a fixed table of pointers to `'static` byte strings.
unsafe impl Sync for FeatureList {}

static FEATURES: FeatureList = FeatureList([
    FEATURE_AUDIO_EFFECT.as_ptr() as *const c_char,
    FEATURE_STEREO.as_ptr() as *const c_char,
    std::ptr::null(),
]);

// ── Parameters ──────────────────────────────────────────────────────────────

/// The most parameters either fixture plugin declares; instance storage is a
/// fixed array so the audio thread never allocates.
const MAX_PARAMS: usize = 4;

struct ParamSpec {
    id: u32,
    name: &'static str,
    /// CLAP's grouping path. Used by hosts to group controls.
    module: &'static str,
    min: f64,
    max: f64,
    default: f64,
    flags: u32,
    /// Names for a stepped or enumerated parameter, in value order from `min`.
    labels: &'static [&'static str],
}

static GAIN_PARAMS: [ParamSpec; 4] = [
    ParamSpec {
        id: 0,
        name: "Gain",
        module: "main",
        min: -24.0,
        max: 24.0,
        default: 0.0,
        flags: CLAP_PARAM_IS_AUTOMATABLE,
        labels: &[],
    },
    ParamSpec {
        id: 1,
        name: "Mix",
        module: "main",
        min: 0.0,
        max: 1.0,
        default: 1.0,
        flags: CLAP_PARAM_IS_AUTOMATABLE,
        labels: &[],
    },
    ParamSpec {
        id: 2,
        name: "Mode",
        module: "main",
        min: 0.0,
        max: 2.0,
        default: 0.0,
        flags: CLAP_PARAM_IS_AUTOMATABLE | CLAP_PARAM_IS_STEPPED | CLAP_PARAM_IS_ENUM,
        labels: &["Clean", "Inverted", "Silent"],
    },
    ParamSpec {
        id: 3,
        name: "Bypass",
        module: "main",
        min: 0.0,
        max: 1.0,
        default: 0.0,
        flags: CLAP_PARAM_IS_BYPASS | CLAP_PARAM_IS_STEPPED,
        labels: &["Off", "On"],
    },
];

static TRIM_PARAMS: [ParamSpec; 3] = [
    ParamSpec {
        id: 0,
        name: "Input Trim",
        module: "input",
        min: -24.0,
        max: 24.0,
        default: 0.0,
        flags: CLAP_PARAM_IS_AUTOMATABLE,
        labels: &[],
    },
    ParamSpec {
        id: 1,
        name: "Output Trim",
        module: "output",
        min: -24.0,
        max: 24.0,
        default: 0.0,
        flags: CLAP_PARAM_IS_AUTOMATABLE,
        labels: &[],
    },
    ParamSpec {
        id: 2,
        name: "Bypass",
        module: "output",
        min: 0.0,
        max: 1.0,
        default: 0.0,
        flags: CLAP_PARAM_IS_BYPASS | CLAP_PARAM_IS_STEPPED,
        labels: &["Off", "On"],
    },
];

/// The failing plugin has one control: arm the failure.
static FAULTY_PARAMS: [ParamSpec; 1] = [ParamSpec {
    id: 0,
    name: "Fail",
    module: "main",
    min: 0.0,
    max: 1.0,
    default: 0.0,
    flags: CLAP_PARAM_IS_AUTOMATABLE | CLAP_PARAM_IS_STEPPED,
    labels: &["Off", "On"],
}];

// ── Plugin identity ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Gain = 0,
    Trim = 1,
    Faulty = 2,
}

impl Kind {
    fn specs(self) -> &'static [ParamSpec] {
        match self {
            Kind::Gain => &GAIN_PARAMS,
            Kind::Trim => &TRIM_PARAMS,
            Kind::Faulty => &FAULTY_PARAMS,
        }
    }

    fn descriptor(self) -> *const clap_plugin_descriptor {
        match self {
            Kind::Gain => &GAIN_DESC,
            Kind::Trim => &TRIM_DESC,
            Kind::Faulty => &FAULTY_DESC,
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        if id == bare_text(GAIN_ID) {
            Some(Kind::Gain)
        } else if id == bare_text(TRIM_ID) {
            Some(Kind::Trim)
        } else if id == bare_text(FAULTY_ID) {
            Some(Kind::Faulty)
        } else {
            None
        }
    }

    fn from_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(Kind::Gain),
            1 => Some(Kind::Trim),
            2 => Some(Kind::Faulty),
            _ => None,
        }
    }
}

/// The text of a NUL-terminated byte string, without its terminator.
fn bare_text(bytes: &'static [u8]) -> &'static str {
    std::str::from_utf8(&bytes[..bytes.len().saturating_sub(1)]).unwrap_or("")
}

static GAIN_DESC: clap_plugin_descriptor = clap_plugin_descriptor {
    clap_version: CLAP_VERSION,
    id: GAIN_ID.as_ptr() as *const c_char,
    name: cstr!("Fixture Gain").as_ptr() as *const c_char,
    vendor: VENDOR.as_ptr() as *const c_char,
    url: URL.as_ptr() as *const c_char,
    manual_url: std::ptr::null(),
    support_url: std::ptr::null(),
    version: FIXTURE_VERSION.as_ptr() as *const c_char,
    description: cstr!("A test fixture: stereo gain with a dry/wet blend.").as_ptr()
        as *const c_char,
    features: FEATURES.0.as_ptr(),
};

static TRIM_DESC: clap_plugin_descriptor = clap_plugin_descriptor {
    clap_version: CLAP_VERSION,
    id: TRIM_ID.as_ptr() as *const c_char,
    name: cstr!("Fixture Trim").as_ptr() as *const c_char,
    vendor: VENDOR.as_ptr() as *const c_char,
    url: URL.as_ptr() as *const c_char,
    manual_url: std::ptr::null(),
    support_url: std::ptr::null(),
    version: FIXTURE_VERSION.as_ptr() as *const c_char,
    description: cstr!("A test fixture: input and output trim.").as_ptr() as *const c_char,
    features: FEATURES.0.as_ptr(),
};

static FAULTY_DESC: clap_plugin_descriptor = clap_plugin_descriptor {
    clap_version: CLAP_VERSION,
    id: FAULTY_ID.as_ptr() as *const c_char,
    name: cstr!("Fixture Faulty").as_ptr() as *const c_char,
    vendor: VENDOR.as_ptr() as *const c_char,
    url: URL.as_ptr() as *const c_char,
    manual_url: std::ptr::null(),
    support_url: std::ptr::null(),
    version: FIXTURE_VERSION.as_ptr() as *const c_char,
    description: cstr!("A test fixture: passes audio through until armed, then fails mid-block.")
        .as_ptr() as *const c_char,
    features: FEATURES.0.as_ptr(),
};

// ── The instance ────────────────────────────────────────────────────────────

#[repr(C)]
struct Instance {
    /// Must stay first: the host's `*const clap_plugin` is this address, and
    /// `destroy` turns it back into a `Box<Instance>`.
    plugin: clap_plugin,
    kind: Kind,
    sample_rate: f64,
    active: bool,
    processing: bool,
    /// Preallocated at `activate`, so `process` never allocates.
    scratch: Vec<f32>,
    values: [f64; MAX_PARAMS],
}

/// Safety: `plugin` must be a `*const clap_plugin` this library handed out,
/// still alive, i.e. between `create_plugin` and `destroy`.
unsafe fn instance<'a>(plugin: *const clap_plugin) -> &'a mut Instance {
    &mut *((*plugin).plugin_data as *mut Instance)
}

fn default_values(kind: Kind) -> [f64; MAX_PARAMS] {
    let mut values = [0.0f64; MAX_PARAMS];
    for (index, spec) in kind.specs().iter().enumerate() {
        values[index] = spec.default;
    }
    values
}

fn param_index(inst: &Instance, id: u32) -> Option<usize> {
    inst.kind.specs().iter().position(|spec| spec.id == id)
}

fn set_param(inst: &mut Instance, id: u32, value: f64) {
    if let Some(index) = param_index(inst, id) {
        let spec = &inst.kind.specs()[index];
        inst.values[index] = value.clamp(spec.min, spec.max);
    }
}

fn is_bypassed(inst: &Instance) -> bool {
    inst.kind
        .specs()
        .iter()
        .position(|spec| spec.flags & CLAP_PARAM_IS_BYPASS != 0)
        .map(|index| inst.values[index] >= 0.5)
        .unwrap_or(false)
}

/// Per-block coefficients, computed once and applied per sample.
#[derive(Clone, Copy)]
enum Coeffs {
    Gain {
        gain: f32,
        mix: f32,
        invert: bool,
        silent: bool,
    },
    Trim {
        gain: f32,
    },
}

fn coefficients(inst: &Instance) -> Coeffs {
    match inst.kind {
        Kind::Gain => Coeffs::Gain {
            gain: db_to_linear(inst.values[0]),
            mix: inst.values[1].clamp(0.0, 1.0) as f32,
            invert: inst.values[2].round() as i64 == 1,
            silent: inst.values[2].round() as i64 == 2,
        },
        Kind::Trim => Coeffs::Trim {
            gain: db_to_linear(inst.values[0] + inst.values[1]),
        },
        // Unarmed, the faulty plugin is a wire.
        Kind::Faulty => Coeffs::Gain {
            gain: 1.0,
            mix: 1.0,
            invert: false,
            silent: false,
        },
    }
}

fn apply(coeffs: Coeffs, x: f32) -> f32 {
    match coeffs {
        Coeffs::Gain {
            gain,
            mix,
            invert,
            silent,
        } => {
            let wet = if silent {
                0.0
            } else if invert {
                -x * gain
            } else {
                x * gain
            };
            x + (wet - x) * mix
        }
        Coeffs::Trim { gain } => x * gain,
    }
}

fn db_to_linear(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

/// Copy a Rust string into one of CLAP's fixed-size inline arrays.
fn write_inline(buffer: &mut [c_char], text: &str) {
    if buffer.is_empty() {
        return;
    }
    let bytes = text.as_bytes();
    let take = bytes.len().min(buffer.len() - 1);
    for (slot, byte) in buffer.iter_mut().zip(bytes).take(take) {
        *slot = *byte as c_char;
    }
    buffer[take] = 0;
}

// ── Plugin vtable ───────────────────────────────────────────────────────────

unsafe extern "C" fn plugin_init(_plugin: *const clap_plugin) -> bool {
    true
}

unsafe extern "C" fn plugin_destroy(plugin: *const clap_plugin) {
    if plugin.is_null() {
        return;
    }
    // Safety: the host calls this exactly once, with a pointer `create_plugin`
    // returned.
    drop(Box::from_raw(plugin as *mut Instance));
}

unsafe extern "C" fn plugin_activate(
    plugin: *const clap_plugin,
    sample_rate: f64,
    _min_frames: u32,
    max_frames: u32,
) -> bool {
    if plugin.is_null() || sample_rate <= 0.0 || max_frames == 0 {
        return false;
    }
    let inst = instance(plugin);
    inst.sample_rate = sample_rate;
    inst.scratch = vec![0.0f32; max_frames as usize];
    inst.active = true;
    true
}

unsafe extern "C" fn plugin_deactivate(plugin: *const clap_plugin) {
    if plugin.is_null() {
        return;
    }
    let inst = instance(plugin);
    inst.active = false;
    inst.processing = false;
}

unsafe extern "C" fn plugin_start_processing(plugin: *const clap_plugin) -> bool {
    if plugin.is_null() {
        return false;
    }
    instance(plugin).processing = true;
    true
}

unsafe extern "C" fn plugin_stop_processing(plugin: *const clap_plugin) {
    if plugin.is_null() {
        return;
    }
    instance(plugin).processing = false;
}

unsafe extern "C" fn plugin_reset(_plugin: *const clap_plugin) {}

unsafe extern "C" fn plugin_on_main_thread(_plugin: *const clap_plugin) {}

/// Read the parameter events the host queued for this block.
///
/// Safety: `list` is a host-owned `clap_input_events`.
unsafe fn apply_param_events(inst: &mut Instance, list: *const clap_input_events) {
    if list.is_null() {
        return;
    }
    let callbacks = &*list;
    let (Some(size), Some(get)) = (callbacks.size, callbacks.get) else {
        return;
    };
    for index in 0..size(list) {
        let header = get(list, index);
        if header.is_null() {
            continue;
        }
        if (*header).type_ == CLAP_EVENT_PARAM_VALUE
            && (*header).space_id == CLAP_CORE_EVENT_SPACE_ID
        {
            let event = &*(header as *const clap_event_param_value);
            set_param(inst, event.param_id, event.value);
        }
    }
}

unsafe extern "C" fn plugin_process(
    plugin: *const clap_plugin,
    process: *const clap_process,
) -> clap_process_status {
    if plugin.is_null() || process.is_null() {
        return CLAP_PROCESS_CONTINUE;
    }
    let inst = instance(plugin);
    let call = &*process;

    apply_param_events(inst, call.in_events);

    if !inst.active || !inst.processing || is_bypassed(inst) {
        return CLAP_PROCESS_CONTINUE;
    }
    if call.audio_outputs.is_null() || call.audio_outputs_count == 0 {
        return CLAP_PROCESS_CONTINUE;
    }

    let out = &*call.audio_outputs;
    if out.data32.is_null() || out.channel_count == 0 {
        return CLAP_PROCESS_CONTINUE;
    }

    let input = if call.audio_inputs_count > 0 && !call.audio_inputs.is_null() {
        Some(&*call.audio_inputs)
    } else {
        None
    };

    // A host may hand us a block longer than it activated us for. Processing
    // what fits beats writing past the scratch buffer; overflowing would be an
    // out-of-bounds write on the audio thread.
    let frames = (call.frames_count as usize).min(inst.scratch.len());
    if frames == 0 {
        return CLAP_PROCESS_CONTINUE;
    }

    if inst.kind == Kind::Faulty && inst.values[0] >= 0.5 {
        // A half-processed block is what a plugin crash looks like from the
        // host's side: write a sentinel over the buffers and fail. The host
        // must discard this, not pass it through.
        for channel in 0..out.channel_count as usize {
            let destination = *out.data32.add(channel);
            if destination.is_null() {
                continue;
            }
            for frame in 0..frames {
                *destination.add(frame) = 0.75;
            }
        }
        return CLAP_PROCESS_ERROR;
    }

    let coeffs = coefficients(inst);

    for channel in 0..out.channel_count as usize {
        let destination = *out.data32.add(channel);
        if destination.is_null() {
            continue;
        }
        let source = input.and_then(|bus| {
            if bus.data32.is_null() || channel >= bus.channel_count as usize {
                return None;
            }
            let pointer = *bus.data32.add(channel);
            if pointer.is_null() {
                None
            } else {
                Some(pointer)
            }
        });

        // Read the whole frame range before writing any of it. The host
        // routinely points input and output at the same memory, and reading
        // through one slice while writing through another is aliasing that
        // cannot be proven away — reading into scratch first sidesteps it.
        for (offset, slot) in inst.scratch.iter_mut().enumerate().take(frames) {
            *slot = match source {
                Some(pointer) => *pointer.add(offset),
                None => *destination.add(offset),
            };
        }
        for slot in inst.scratch.iter_mut().take(frames) {
            *slot = apply(coeffs, *slot);
        }
        for (offset, value) in inst.scratch.iter().enumerate().take(frames) {
            *destination.add(offset) = *value;
        }
    }

    CLAP_PROCESS_CONTINUE
}

// ── The `audio-ports` extension ─────────────────────────────────────────────

unsafe extern "C" fn audio_port_count(_plugin: *const clap_plugin, _is_input: bool) -> u32 {
    1
}

unsafe extern "C" fn audio_port_get(
    plugin: *const clap_plugin,
    index: u32,
    is_input: bool,
    info: *mut clap_audio_port_info,
) -> bool {
    if plugin.is_null() || info.is_null() || index != 0 {
        return false;
    }
    let mut declared = std::mem::zeroed::<clap_audio_port_info>();
    declared.id = if is_input { 1 } else { 2 };
    declared.flags = CLAP_AUDIO_PORT_IS_MAIN;
    declared.channel_count = 2;
    declared.port_type = CLAP_PORT_STEREO.as_ptr();
    // The Trim plugin reserved separate output buffers, which the
    // `in_place_pair` field is how a plugin says: the host may not write into
    // the input. The other two permit in-place processing.
    declared.in_place_pair = if is_input || instance(plugin).kind == Kind::Trim {
        CLAP_INVALID_ID
    } else {
        1
    };
    *info = declared;
    true
}

static AUDIO_PORTS: Shared<clap_plugin_audio_ports> = Shared(clap_plugin_audio_ports {
    count: Some(audio_port_count),
    get: Some(audio_port_get),
});

unsafe extern "C" fn plugin_get_extension(
    _plugin: *const clap_plugin,
    id: *const c_char,
) -> *const c_void {
    if id.is_null() {
        return std::ptr::null();
    }
    let id = CStr::from_ptr(id);
    if id == CLAP_EXT_AUDIO_PORTS {
        return &AUDIO_PORTS.0 as *const clap_plugin_audio_ports as *const c_void;
    }
    if id == CLAP_EXT_PARAMS {
        return &PARAMS.0 as *const clap_plugin_params as *const c_void;
    }
    if id == CLAP_EXT_STATE {
        return &STATE.0 as *const clap_plugin_state as *const c_void;
    }
    std::ptr::null()
}

// ── The `params` extension ──────────────────────────────────────────────────

unsafe extern "C" fn params_count(plugin: *const clap_plugin) -> u32 {
    if plugin.is_null() {
        return 0;
    }
    instance(plugin).kind.specs().len() as u32
}

unsafe extern "C" fn params_get_info(
    plugin: *const clap_plugin,
    index: u32,
    out: *mut clap_param_info,
) -> bool {
    if plugin.is_null() || out.is_null() {
        return false;
    }
    let specs = instance(plugin).kind.specs();
    let Some(spec) = specs.get(index as usize) else {
        return false;
    };
    let info = &mut *out;
    info.id = spec.id;
    info.flags = spec.flags;
    // The host passes this cookie back; for a fixture the index is enough.
    info.cookie = index as *mut c_void;
    write_inline(&mut info.name, spec.name);
    write_inline(&mut info.module, spec.module);
    info.min_value = spec.min;
    info.max_value = spec.max;
    info.default_value = spec.default;
    true
}

unsafe extern "C" fn params_get_value(plugin: *const clap_plugin, id: u32, out: *mut f64) -> bool {
    if plugin.is_null() || out.is_null() {
        return false;
    }
    let inst = instance(plugin);
    let Some(index) = param_index(inst, id) else {
        return false;
    };
    *out = inst.values[index];
    true
}

unsafe extern "C" fn params_value_to_text(
    plugin: *const clap_plugin,
    id: u32,
    value: f64,
    buffer: *mut c_char,
    capacity: u32,
) -> bool {
    if plugin.is_null() || buffer.is_null() || capacity == 0 {
        return false;
    }
    let inst = instance(plugin);
    let Some(index) = param_index(inst, id) else {
        return false;
    };
    let spec = &inst.kind.specs()[index];
    let text = match label_for(spec, value) {
        Some(label) => label.to_string(),
        None => format!("{value:.2}"),
    };
    // Safety: the host promises `capacity` writable bytes at `buffer`.
    let out = std::slice::from_raw_parts_mut(buffer, capacity as usize);
    write_inline(out, &text);
    true
}

unsafe extern "C" fn params_text_to_value(
    plugin: *const clap_plugin,
    id: u32,
    text: *const c_char,
    out: *mut f64,
) -> bool {
    if plugin.is_null() || text.is_null() || out.is_null() {
        return false;
    }
    let inst = instance(plugin);
    let Some(index) = param_index(inst, id) else {
        return false;
    };
    let spec = &inst.kind.specs()[index];
    let raw = CStr::from_ptr(text).to_string_lossy();
    let trimmed = raw.trim();
    if let Some(position) = spec
        .labels
        .iter()
        .position(|label| label.eq_ignore_ascii_case(trimmed))
    {
        *out = spec.min + position as f64;
        return true;
    }
    match trimmed.parse::<f64>() {
        Ok(value) => {
            *out = value.clamp(spec.min, spec.max);
            true
        }
        Err(_) => false,
    }
}

unsafe extern "C" fn params_flush(
    plugin: *const clap_plugin,
    input: *const clap_input_events,
    _output: *const clap_output_events,
) {
    if plugin.is_null() {
        return;
    }
    apply_param_events(instance(plugin), input);
}

fn label_for(spec: &ParamSpec, value: f64) -> Option<&'static str> {
    if spec.labels.is_empty() {
        return None;
    }
    let index = (value - spec.min).round();
    if !(0.0..=f64::MAX).contains(&index) {
        return None;
    }
    spec.labels.get(index as usize).copied()
}

static PARAMS: Shared<clap_plugin_params> = Shared(clap_plugin_params {
    count: Some(params_count),
    get_info: Some(params_get_info),
    get_value: Some(params_get_value),
    value_to_text: Some(params_value_to_text),
    text_to_value: Some(params_text_to_value),
    flush: Some(params_flush),
});

// ── The `state` extension ───────────────────────────────────────────────────

/// Layout of the state blob: a kind tag, a value count, then the values.
///
/// The tag is written so `load` can refuse a blob that belongs to the *other*
/// plugin in this library. A state blob must not be able to turn one plugin
/// into another.
const STATE_HEADER: usize = 8;
const STATE_LEN: usize = STATE_HEADER + 8 * MAX_PARAMS;

unsafe extern "C" fn state_save(plugin: *const clap_plugin, stream: *const clap_ostream) -> bool {
    if plugin.is_null() || stream.is_null() {
        return false;
    }
    let inst = instance(plugin);
    let Some(write) = (*stream).write else {
        return false;
    };

    let mut blob = [0u8; STATE_LEN];
    blob[0..4].copy_from_slice(&(inst.kind as u32).to_le_bytes());
    blob[4..8].copy_from_slice(&(MAX_PARAMS as u32).to_le_bytes());
    for (index, value) in inst.values.iter().enumerate() {
        let start = STATE_HEADER + index * 8;
        blob[start..start + 8].copy_from_slice(&value.to_le_bytes());
    }

    let written = write(stream, blob.as_ptr() as *const c_void, STATE_LEN as u64);
    written == STATE_LEN as i64
}

unsafe extern "C" fn state_load(plugin: *const clap_plugin, stream: *const clap_istream) -> bool {
    if plugin.is_null() || stream.is_null() {
        return false;
    }
    let inst = instance(plugin);
    let Some(read) = (*stream).read else {
        return false;
    };

    let mut blob = [0u8; STATE_LEN];
    let got = read(stream, blob.as_mut_ptr() as *mut c_void, STATE_LEN as u64);
    if got < STATE_HEADER as i64 {
        return false;
    }
    let got = got as usize;

    let tagged = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
    if tagged != inst.kind as u32 {
        return false;
    }

    let count = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]) as usize;
    for index in 0..count.min(MAX_PARAMS) {
        let start = STATE_HEADER + index * 8;
        if start + 8 > got {
            break;
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&blob[start..start + 8]);
        inst.values[index] = f64::from_le_bytes(bytes);
    }
    true
}

static STATE: Shared<clap_plugin_state> = Shared(clap_plugin_state {
    save: Some(state_save),
    load: Some(state_load),
});

// ── Factory and entry point ─────────────────────────────────────────────────

const PLUGIN_COUNT: u32 = 3;

unsafe extern "C" fn factory_count(_factory: *const clap_plugin_factory) -> u32 {
    PLUGIN_COUNT
}

unsafe extern "C" fn factory_descriptor(
    _factory: *const clap_plugin_factory,
    index: u32,
) -> *const clap_plugin_descriptor {
    match Kind::from_index(index) {
        Some(kind) => kind.descriptor(),
        None => std::ptr::null(),
    }
}

unsafe extern "C" fn factory_create(
    _factory: *const clap_plugin_factory,
    _host: *const clap_host,
    plugin_id: *const c_char,
) -> *const clap_plugin {
    if plugin_id.is_null() {
        return std::ptr::null();
    }
    let id = CStr::from_ptr(plugin_id).to_string_lossy();
    // The plugin uses no host extensions, so the host pointer is not kept.
    let Some(kind) = Kind::from_id(&id) else {
        return std::ptr::null();
    };

    let mut boxed = Box::new(Instance {
        plugin: clap_plugin {
            desc: kind.descriptor(),
            plugin_data: std::ptr::null_mut(),
            init: Some(plugin_init),
            destroy: Some(plugin_destroy),
            activate: Some(plugin_activate),
            deactivate: Some(plugin_deactivate),
            start_processing: Some(plugin_start_processing),
            stop_processing: Some(plugin_stop_processing),
            reset: Some(plugin_reset),
            process: Some(plugin_process),
            get_extension: Some(plugin_get_extension),
            on_main_thread: Some(plugin_on_main_thread),
        },
        kind,
        sample_rate: 0.0,
        active: false,
        processing: false,
        scratch: Vec::new(),
        values: default_values(kind),
    });

    // The host reaches the instance through `plugin_data`. Taking the address
    // first ends the borrow, so the assignment is not a borrow conflict.
    let address: *mut Instance = &mut *boxed;
    boxed.plugin.plugin_data = address as *mut c_void;

    // `Instance` is `repr(C)` with `plugin` first, so this pointer is also the
    // address of the embedded `clap_plugin`.
    Box::into_raw(boxed) as *const clap_plugin
}

static FACTORY: Shared<clap_plugin_factory> = Shared(clap_plugin_factory {
    get_plugin_count: Some(factory_count),
    get_plugin_descriptor: Some(factory_descriptor),
    create_plugin: Some(factory_create),
});

/// How many times a host has initialised this library without deinitialising.
/// A real plugin uses this to refuse to die while an instance is alive.
static INIT_COUNT: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn entry_init(_plugin_path: *const c_char) -> bool {
    INIT_COUNT.fetch_add(1, Ordering::SeqCst);
    true
}

/// Total `deinit` calls over this library's lifetime.
///
/// A host that unloads the library must have called `deinit` once per `init`;
/// tests read this over the C ABI to prove the pairing happened. Monotone, so
/// a check of "the count went up" can never fail for a concurrent host.
static DEINIT_COUNT: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn entry_deinit() {
    DEINIT_COUNT.fetch_add(1, Ordering::SeqCst);
    let previous = INIT_COUNT.load(Ordering::SeqCst);
    if previous > 0 {
        INIT_COUNT.store(previous - 1, Ordering::SeqCst);
    }
}

/// Test-only probe: how many times a host has called `entry.deinit()`.
///
/// # Safety
/// Plain C ABI: no arguments, no aliasing concerns.
#[no_mangle]
pub extern "C" fn fixture_deinit_calls() -> u32 {
    DEINIT_COUNT.load(Ordering::SeqCst)
}

unsafe extern "C" fn entry_get_factory(id: *const c_char) -> *const c_void {
    if id.is_null() {
        return std::ptr::null();
    }
    if CStr::from_ptr(id) == CLAP_PLUGIN_FACTORY_ID {
        return &FACTORY.0 as *const clap_plugin_factory as *const c_void;
    }
    std::ptr::null()
}

/// The symbol a CLAP host looks up after opening the library.
///
/// `Shared` is `repr(transparent)`, so this is laid out exactly as the
/// `clap_plugin_entry` the host expects to find.
#[no_mangle]
pub static clap_entry: Shared<clap_plugin_entry> = Shared(clap_plugin_entry {
    clap_version: CLAP_VERSION,
    init: Some(entry_init),
    deinit: Some(entry_deinit),
    get_factory: Some(entry_get_factory),
});
