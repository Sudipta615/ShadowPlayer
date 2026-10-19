//! End-to-end tests for the CLAP host, against the in-repo fixture plugin.
//!
//! The fixture is a separate `cdylib` workspace member, not a dependency: a
//! CLAP plugin can only be tested the way a host loads it, by opening the built
//! library and looking up `clap_entry`. So these tests run the real ABI —
//! enumeration, activation, parameter events, audio, and state — rather than a
//! mock that would agree with whatever the host happens to do.
//!
//! Run `cargo build -p clap-fixture-plugin` before `cargo test --features
//! clap-host`; the test says so if the library is missing.

#![cfg(feature = "clap-host")]

use std::path::PathBuf;
use std::sync::OnceLock;

use engine::dsp::graph2::prod::{ClapPluginHostNode, DspNode};

const GAIN_ID: &str = "com.shadowplayer.fixture.gain";
const TRIM_ID: &str = "com.shadowplayer.fixture.trim";
const FAULTY_ID: &str = "com.shadowplayer.fixture.faulty";
const SAMPLE_RATE: f32 = 48_000.0;

/// Locate the built fixture library, building it if necessary.
///
/// The engine crate is excluded from the repository workspace, so a build
/// invoked from here would look for the fixture in the engine's own target
/// directory while `cargo build -p clap-fixture-plugin` from the repository
/// root puts it in the root one. Both are checked, and the root manifest is
/// used to build it when neither has it.
fn locate_fixture() -> PathBuf {
    let name = format!(
        "libclap_fixture_plugin{}",
        std::env::consts::DLL_SUFFIX
    );

    // A test binary lives in `<target>/<profile>/deps/`.
    let mut test_directory = std::env::current_exe().expect("test binary path");
    test_directory.pop(); // the test binary
    test_directory.pop(); // the `deps` directory
    assert!(test_directory.pop(), "expected a profile directory");

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root");
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    let candidates = [
        test_directory.join(&name),
        root.join("target").join(profile).join(&name),
        root.join("target/debug").join(&name),
    ];
    if let Some(found) = candidates.iter().find(|path| path.is_file()) {
        return found.clone();
    }

    let cargo = option_env!("CARGO").unwrap_or("cargo");
    let status = std::process::Command::new(cargo)
        .args(["build", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .args(["-p", "clap-fixture-plugin"])
        .status()
        .unwrap_or_else(|e| panic!("could not run `{cargo}`: {e}"));
    assert!(status.success(), "building the fixture plugin failed");

    candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .unwrap_or_else(|| {
            panic!("the fixture plugin was built but not found in {candidates:?}")
        })
}

/// The built fixture library, resolved once for the whole test binary.
fn fixture_library() -> PathBuf {
    static FIXTURE: OnceLock<PathBuf> = OnceLock::new();
    FIXTURE.get_or_init(locate_fixture).clone()
}

fn load(kind_id: &str) -> ClapPluginHostNode {
    let mut node = ClapPluginHostNode::new(SAMPLE_RATE);
    node.load_plugin(&fixture_library(), Some(kind_id))
        .unwrap_or_else(|e| panic!("loading {kind_id} failed: {e}"));
    node
}

fn process(node: &mut ClapPluginHostNode, left: &mut [f32], right: &mut [f32]) {
    let mut planes = [&mut left[..], &mut right[..]];
    node.process_block_f32(&mut planes);
}

#[test]
fn discovery_lists_every_plugin_in_the_library_without_instantiating_one() {
    let found = ClapPluginHostNode::discover(&fixture_library()).expect("probe should succeed");

    assert_eq!(found.len(), 3, "the fixture ships three plugins");
    assert_eq!(found[0].id, GAIN_ID);
    assert_eq!(found[0].name, "Fixture Gain");
    assert_eq!(found[0].vendor, "ShadowPlayer");
    assert_eq!(found[0].version, "1.0.0");
    assert!(!found[0].description.is_empty());
    assert_eq!(found[1].id, TRIM_ID);
    assert_eq!(found[1].name, "Fixture Trim");
    assert_eq!(found[2].id, FAULTY_ID);
    assert_eq!(found[2].name, "Fixture Faulty");
}

#[test]
fn a_probed_library_can_be_probed_again() {
    // `discover` initialises and deinitialises the library each time, so a
    // second probe must not trip over a half-torn-down first one.
    let path = fixture_library();
    let first = ClapPluginHostNode::discover(&path).expect("first probe");
    let second = ClapPluginHostNode::discover(&path).expect("second probe");
    assert_eq!(first, second);
}

#[test]
fn loading_reports_the_plugin_and_the_parameters_it_exposes() {
    let node = load(GAIN_ID);
    let report = node.rack_report();

    assert!(report.is_loaded());
    let plugin = report.plugin.as_ref().expect("a descriptor");
    assert_eq!(plugin.name, "Fixture Gain");
    assert_eq!(plugin.id, GAIN_ID);
    assert!(report.has_state, "the fixture implements the state extension");
    assert_eq!(report.latency_samples, 0);
    assert!(!report.bypassed);
    assert!(report.path.is_some());
    assert!(report.error.is_none());

    let names: Vec<&str> = report.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Gain", "Mix", "Mode", "Bypass"]);

    let gain = &report.params[0];
    assert_eq!((gain.min, gain.max), (-24.0, 24.0));
    assert_eq!(gain.value, 0.0, "a fresh instance reports its defaults");
    assert_eq!(gain.module, "main");

    let mix = &report.params[1];
    assert_eq!((mix.min, mix.max, mix.default, mix.value), (0.0, 1.0, 1.0, 1.0));

    let mode = &report.params[2];
    assert!(mode.is_enum && mode.is_stepped, "the mode is an enumeration");

    let bypass = &report.params[3];
    assert!(bypass.is_bypass, "the plugin declares its bypass switch");

    // Read-only and hidden parameters do not exist here, so every one is
    // offered to the user.
    assert_eq!(report.editable_params().count(), 4);
    assert_eq!(report.bypass_param().map(|p| p.id), Some(3));
}

#[test]
fn the_two_plugins_in_the_library_are_not_the_same_plugin() {
    let node = load(TRIM_ID);
    let report = node.rack_report();
    assert_eq!(report.plugin.as_ref().map(|p| p.name.as_str()), Some("Fixture Trim"));

    let names: Vec<&str> = report.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Input Trim", "Output Trim", "Bypass"]);
    assert_eq!(report.bypass_param().map(|p| p.id), Some(2));
}

#[test]
fn an_unknown_plugin_id_is_refused_rather_than_silently_ignored() {
    let mut node = ClapPluginHostNode::new(SAMPLE_RATE);
    let result = node.load_plugin(&fixture_library(), Some("com.shadowplayer.nope"));
    assert!(result.is_err());
    assert!(!node.rack_report().is_loaded());
}

#[test]
fn a_parameter_set_by_the_host_reaches_the_plugin_and_changes_the_audio() {
    let mut node = load(GAIN_ID);
    node.set_param(0, 6.0);

    let mut left = vec![1.0f32; 64];
    let mut right = vec![-0.5f32; 64];
    process(&mut node, &mut left, &mut right);

    let expected = 10f64.powf(6.0 / 20.0) as f32;
    assert!(
        (left[0] - expected).abs() < 1e-4,
        "left {} should be {expected}",
        left[0]
    );
    assert!(
        (right[0] + expected * 0.5).abs() < 1e-4,
        "right {} should be {}",
        right[0],
        -expected * 0.5
    );

    // The plugin now reports the value the host sent. The audio above is the
    // proof the event landed; this is the plugin's own view of it agreeing.
    let gain = node
        .param_infos()
        .into_iter()
        .find(|p| p.id == 0)
        .expect("gain");
    assert!((gain.value - 6.0).abs() < 1e-9);
}

#[test]
fn a_parameter_reaches_the_plugin_without_processing_a_block() {
    // A plugin sees parameter events only inside `process` or `params.flush`.
    // A paused player never processes, so without the flush a preset saved then
    // would record whatever the plugin was activated with rather than what the
    // user set.
    let mut node = load(GAIN_ID);
    node.set_param(0, 6.0);

    let gain = node
        .param_infos()
        .into_iter()
        .find(|p| p.id == 0)
        .expect("gain");
    assert!(
        (gain.value - 6.0).abs() < 1e-9,
        "the plugin reports {} after a flush",
        gain.value
    );

    // The state it would hand back carries the same value, which is what makes
    // a preset's blob worth storing alongside the parameter list.
    let bytes = node.save_state().expect("state save");
    assert!(bytes.len() >= 16, "the fixture writes a header and its values");
    let restored = f64::from_le_bytes(bytes[8..16].try_into().expect("eight bytes"));
    assert!((restored - 6.0).abs() < 1e-9, "the blob holds {restored}");
}

#[test]
fn the_dry_wet_blend_scales_how_much_of_the_gain_is_heard() {
    let mut node = load(GAIN_ID);
    node.set_param(0, 6.0);
    node.set_param(1, 0.5);

    let mut left = vec![1.0f32; 32];
    let mut right = vec![1.0f32; 32];
    process(&mut node, &mut left, &mut right);

    let wet = 10f64.powf(6.0 / 20.0) as f32;
    let expected = 1.0 + (wet - 1.0) * 0.5;
    assert!((left[0] - expected).abs() < 1e-4, "got {}", left[0]);
}

#[test]
fn the_bypass_parameter_passes_audio_through_untouched() {
    let mut node = load(GAIN_ID);
    node.set_param(0, 12.0);
    node.set_param(3, 1.0);

    let mut left = vec![0.25f32; 32];
    let mut right = vec![0.25f32; 32];
    process(&mut node, &mut left, &mut right);

    assert!((left[0] - 0.25).abs() < 1e-9, "bypassed audio must not change");
}

#[test]
fn state_captures_every_parameter_and_restores_it() {
    let mut node = load(GAIN_ID);
    node.set_param(0, -9.0);
    node.set_param(1, 0.25);
    node.set_param(2, 1.0);

    // Parameters travel to a plugin as events, and a plugin applies them when
    // it next processes — the host cannot rewrite a plugin's values behind its
    // back. So a capture has to come after a block, which is the real ordering
    // a host works in too.
    let mut left = vec![0.0f32; 16];
    let mut right = vec![0.0f32; 16];
    process(&mut node, &mut left, &mut right);

    let blob = node.save_state().expect("state save");
    assert!(!blob.is_empty());

    // Move everything, then put it back.
    node.set_param(0, 21.0);
    node.set_param(1, 1.0);
    node.set_param(2, 0.0);
    process(&mut node, &mut left, &mut right);
    node.load_state(&blob).expect("state load");

    let values: Vec<f64> = node
        .param_infos()
        .into_iter()
        .filter(|p| p.id <= 2)
        .map(|p| p.value)
        .collect();
    assert_eq!(values, vec![-9.0, 0.25, 1.0]);
}

#[test]
fn a_state_blob_from_the_other_plugin_is_refused() {
    let mut gain = load(GAIN_ID);
    gain.set_param(0, 4.0);
    let blob = gain.save_state().expect("state save");

    let mut trim = load(TRIM_ID);
    assert!(
        trim.load_state(&blob).is_err(),
        "a state blob must not be able to turn one plugin into another"
    );
}

#[test]
fn processing_an_empty_block_is_not_a_crash() {
    let mut node = load(GAIN_ID);
    let mut left: Vec<f32> = Vec::new();
    let mut right: Vec<f32> = Vec::new();
    process(&mut node, &mut left, &mut right);
}

#[test]
fn the_node_degrades_to_a_pass_through_when_bypassed_by_the_host() {
    let mut node = load(GAIN_ID);
    node.set_param(0, 20.0);
    node.set_enabled(false);

    let mut left = vec![0.1f32; 16];
    let mut right = vec![0.1f32; 16];
    process(&mut node, &mut left, &mut right);

    assert!((left[0] - 0.1).abs() < 1e-9, "a disabled host node is not in the chain");
    assert!(node.rack_report().bypassed);
}

#[test]
fn a_64_bit_block_crosses_the_f32_plugin_abi_without_losing_its_gain() {
    // The node declares F32 precision to the graph, but a graph that asks for
    // a f64 block anyway must get the plugin's processing, not a silent copy.
    let mut node = load(GAIN_ID);
    node.set_param(0, 6.0);

    let mut left = vec![1.0f64; 48];
    let mut right = vec![-1.0f64; 48];
    let mut planes = [&mut left[..], &mut right[..]];
    node.process_block_f64(&mut planes);

    let expected = 10f64.powf(6.0 / 20.0);
    assert!(
        (left[0] - expected).abs() < 1e-4,
        "left {} should be {expected}",
        left[0]
    );
    assert!(
        (right[0] + expected).abs() < 1e-4,
        "right {} should be {}",
        right[0],
        -expected
    );
}

#[test]
fn a_plugin_that_reserved_separate_buffers_is_not_processed_in_place() {
    // The Trim fixture declares its main ports without an `in_place_pair`, so
    // the host must stage the output instead of handing the input buffers back
    // as outputs.
    let mut node = load(TRIM_ID);
    node.set_param(0, 6.0);

    let mut left = vec![1.0f32; 32];
    let mut right = vec![1.0f32; 32];
    process(&mut node, &mut left, &mut right);

    let expected = 10f32.powf(6.0 / 20.0);
    assert!(
        (left[0] - expected).abs() < 1e-4,
        "the staged output must reach the graph, got {}",
        left[0]
    );
}

#[test]
fn a_plugin_that_fails_mid_block_is_taken_out_of_the_chain() {
    let mut node = load(FAULTY_ID);
    node.set_param(0, 1.0); // arm the failure

    let mut left = vec![0.25f32; 32];
    let mut right = vec![0.25f32; 32];
    process(&mut node, &mut left, &mut right);

    // The plugin wrote its sentinel (0.75) over the block before returning
    // CLAP_PROCESS_ERROR; the host must discard that, not pass it on.
    assert!(
        left.iter().all(|sample| *sample == 0.0),
        "a failed block must be silenced, got {}",
        left[0]
    );
    assert!(right.iter().all(|sample| *sample == 0.0));

    let report = node.rack_report();
    assert!(report.bypassed, "a failed plugin leaves the chain");
    assert!(
        report
            .error
            .as_deref()
            .is_some_and(|text| text.contains("CLAP_PROCESS_ERROR")),
        "the report must say what happened: {:?}",
        report.error
    );
    assert!(!node.is_active());

    // Once failed, the plugin is not called again: the dry signal passes.
    node.set_param(0, 0.0);
    let mut left = vec![0.25f32; 32];
    let mut right = vec![-0.25f32; 32];
    process(&mut node, &mut left, &mut right);
    assert_eq!(left[0], 0.25, "a plugin out of the chain is a wire");
    assert_eq!(right[0], -0.25);

    // Re-enabling is the explicit retry; it clears the latch, and a plugin
    // that fails again latches again.
    node.set_enabled(true);
    assert!(node.rack_report().error.is_none());

    node.set_param(0, 1.0);
    let mut left = vec![0.25f32; 32];
    let mut right = vec![0.25f32; 32];
    process(&mut node, &mut left, &mut right);
    assert!(left.iter().all(|sample| *sample == 0.0));
    assert!(!node.is_active(), "the latch comes back on the next failure");
}

#[test]
fn dropping_a_node_deinitialises_the_library_it_unloaded() {
    // `entry.deinit` pairs the `init` a load performs; a host that only ever
    // unloads the library leaks the library's initialisation. The fixture
    // counts its deinit calls, and the count is monotone, so a concurrent test
    // can only make this check pass more easily — never fail spuriously.
    let path = fixture_library();
    let deinit_calls = |path: &PathBuf| -> u32 {
        unsafe {
            let library = libloading::Library::new(path).expect("fixture library");
            let probe: libloading::Symbol<unsafe extern "C" fn() -> u32> = library
                .get(b"fixture_deinit_calls\0")
                .expect("deinit probe symbol");
            probe()
        }
    };

    let before = deinit_calls(&path);
    {
        let _node = load(GAIN_ID);
    }
    let after = deinit_calls(&path);
    assert!(
        after > before,
        "dropping the node must call entry.deinit(), saw {before} then {after}"
    );
}
