//! The backend picker must offer exactly what this build can open.
//!
//! `available_backends()` is the list a host shows its user. Two mistakes
//! are possible and both are bad: offering a backend the build cannot open
//! (the user picks it and gets a failure or a silent downgrade), and hiding
//! one it can (the user has hardware the engine could drive but is never
//! told about it).
//!
//! This suite checks the list against the compiled features and the target
//! operating system, and checks that the two backends the engine declares
//! but has never implemented — PipeWire and JACK — are absent from it.

use std::sync::Arc;

use engine::buffer::FixedFrameBuffer;
use engine::output::available_backends;
use engine::AudioBackend;

#[test]
fn auto_is_always_the_first_entry() {
    let backends = available_backends();
    assert_eq!(
        backends.first(),
        Some(&AudioBackend::Auto),
        "Auto is the only entry that is meaningful everywhere, so it must be first"
    );
    #[cfg(any(
        target_os = "linux",
        target_os = "macos",
        all(
            target_os = "windows",
            any(feature = "wasapi-native", feature = "asio-native")
        ),
    ))]
    assert!(
        backends.len() >= 2,
        "the picker must offer at least one platform backend besides Auto"
    );
}

#[test]
fn the_picker_never_offers_the_same_backend_twice() {
    let backends = available_backends();
    for (i, backend) in backends.iter().enumerate() {
        assert!(
            !backends[..i].contains(backend),
            "{backend:?} is listed twice"
        );
    }
}

#[test]
fn the_exclusive_backend_for_this_operating_system_is_offered() {
    let backends = available_backends();

    #[cfg(target_os = "linux")]
    {
        assert!(
            backends.contains(&AudioBackend::ExclusiveAlsa),
            "Linux must offer the direct ALSA backend"
        );
    }
    #[cfg(target_os = "macos")]
    {
        assert!(
            backends.contains(&AudioBackend::ExclusiveCoreAudioHog),
            "macOS must offer the CoreAudio hog-mode backend"
        );
    }
    #[cfg(target_os = "windows")]
    {
        // On Windows the exclusive backends are feature-gated: the native
        // WASAPI and ASIO implementations are opt-in.
        #[cfg(feature = "wasapi-native")]
        assert!(
            backends.contains(&AudioBackend::ExclusiveWasapi),
            "the wasapi-native feature must make the WASAPI exclusive backend \
             available"
        );
        #[cfg(not(feature = "wasapi-native"))]
        assert!(
            !backends.contains(&AudioBackend::ExclusiveWasapi),
            "the WASAPI exclusive backend needs the wasapi-native feature"
        );
        #[cfg(feature = "asio-native")]
        assert!(
            backends.contains(&AudioBackend::ExclusiveAsio),
            "the asio-native feature must make the ASIO backend available"
        );
        #[cfg(not(feature = "asio-native"))]
        assert!(
            !backends.contains(&AudioBackend::ExclusiveAsio),
            "the ASIO backend needs the asio-native feature"
        );
    }
}

#[test]
fn the_backends_of_other_operating_systems_are_never_offered() {
    let backends = available_backends();

    #[cfg(not(target_os = "windows"))]
    {
        assert!(
            !backends.contains(&AudioBackend::ExclusiveWasapi),
            "the WASAPI exclusive backend is Windows-only"
        );
        assert!(
            !backends.contains(&AudioBackend::ExclusiveAsio),
            "the ASIO backend is Windows-only"
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert!(
            !backends.contains(&AudioBackend::ExclusiveCoreAudioHog),
            "the CoreAudio hog-mode backend is macOS-only"
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        assert!(
            !backends.contains(&AudioBackend::ExclusiveAlsa),
            "the direct ALSA backend is Linux-only"
        );
    }
}

#[test]
fn the_backends_that_were_never_implemented_are_not_offered() {
    // PipeWire and JACK are declared in the settings format, but the engine
    // has no implementation for either in any configuration. Offering them
    // would be a promise the engine cannot keep.
    let backends = available_backends();
    assert!(
        !backends.contains(&AudioBackend::PipeWire),
        "PipeWire has no implementation and must not be offered"
    );
    assert!(
        !backends.contains(&AudioBackend::Jack),
        "JACK has no implementation and must not be offered"
    );
}

#[test]
fn the_null_backend_is_not_offered_in_the_picker() {
    // The null sink discards the audio. It is a headless and CI facility,
    // not something a user should be able to pick and wonder why there is no
    // sound.
    let backends = available_backends();
    assert!(
        !backends.contains(&AudioBackend::Null),
        "the null sink must not appear in the user-facing picker"
    );
}

#[test]
fn selecting_an_unimplemented_backend_is_refused_under_the_strict_policy() {
    // The picker does not offer them, but a configuration file can still name
    // one. Under the strict policy that must be a refusal, not a silent
    // downgrade to the shared output.
    let buffer = Arc::new(FixedFrameBuffer::new(1024).expect("a ring buffer must be creatable"));
    for backend in [AudioBackend::PipeWire, AudioBackend::Jack] {
        let result = engine::output::create_output(
            Arc::clone(&buffer),
            backend,
            None,
            config::FallbackPolicy::Strict,
        );
        assert!(
            result.is_err(),
            "{backend:?} has no implementation and must be refused under the \
             strict fallback policy"
        );
        let error = result.err().expect("the call must have failed");
        let message = error.to_string();
        assert!(
            message.contains("PipeWire") || message.contains("JACK"),
            "the refusal must name the backend, it said: {message}"
        );
    }
}

#[test]
fn the_picker_lists_exactly_the_backends_this_build_can_open() {
    let backends = available_backends();

    // The expected list, spelled out per platform and feature set. A backend
    // added to the picker without being added here fails this test, which is
    // the point: the picker and the build must not drift apart.
    let mut expected: Vec<AudioBackend> = vec![AudioBackend::Auto];
    #[cfg(target_os = "linux")]
    expected.push(AudioBackend::ExclusiveAlsa);
    #[cfg(all(target_os = "windows", feature = "wasapi-native"))]
    expected.push(AudioBackend::ExclusiveWasapi);
    #[cfg(all(target_os = "windows", feature = "asio-native"))]
    expected.push(AudioBackend::ExclusiveAsio);
    #[cfg(target_os = "macos")]
    expected.push(AudioBackend::ExclusiveCoreAudioHog);

    assert_eq!(
        backends, expected,
        "the picker must list exactly the backends this build can open"
    );
}
