//! HRTF rendering: the output must be finite, audible, and honest about
//! which head model is in use.
//!
//! The engine has two head models: the analytic one (Woodworth ITD +
//! Duda-Martens head shadow + pinna notch) and the spectral one (a measured
//! or synthetic HRTF dataset convolved per direction). A renderer that
//! silently fell back from one to the other would produce plausible audio
//! and the wrong image, so the fallback has to be *reported* — and this suite
//! checks that it is.

use std::sync::Arc;

use engine::spatial::hrtf::Ear;
use engine::spatial::provider::{HrtfDatasetProvider, HrtfProvider};
use engine::spatial::{BinauralRenderer, HrtfDataset, SpatialRenderer, SpeakerLayout, Vec3};

/// Sample rate for the rendering tests.
const SR: u32 = 48_000;

/// A synthetic HRTF dataset on a 30° × 30° grid with 32 taps per ear.
fn synthetic_dataset() -> HrtfDataset {
    HrtfDataset::synthetic(SR, 32, 30.0, 30.0)
}

/// A unit vector at `azimuth_deg` azimuth and `elevation_deg` elevation.
fn direction(azimuth_deg: f32, elevation_deg: f32) -> Vec3 {
    let az = azimuth_deg.to_radians();
    let el = elevation_deg.to_radians();
    let horiz = el.cos();
    Vec3::new(az.sin() * horiz, az.cos() * horiz, el.sin())
}

/// Render a 1 kHz tone through the binaural renderer with one object at
/// `direction`, and return the interleaved stereo output.
fn render(renderer: &mut BinauralRenderer, direction: Vec3) -> Vec<f32> {
    let mut scene = engine::spatial::SpatialScene::new(SR);
    let id = scene
        .create_audio_object(direction)
        .expect("creating a spatial object must succeed");
    assert!(id.0 < engine::spatial::MAX_SPATIAL_OBJECTS);

    let frames = 1024usize;
    let mut input = Vec::with_capacity(frames);
    for i in 0..frames {
        input.push((2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR as f32).sin() * 0.5);
    }
    let mut out = vec![0.0f32; 2 * frames];
    renderer
        .process_block(&scene, &[&input], frames, &mut out)
        .expect("the binaural renderer must accept a stereo layout and one object");
    out
}

#[test]
fn the_interpolated_response_is_finite_and_not_silent() {
    let dataset = synthetic_dataset();
    let provider = HrtfDatasetProvider::new(dataset);
    assert_eq!(
        provider.taps(),
        32,
        "the provider must report the tap count"
    );

    let mut left = vec![0.0f32; 32];
    let mut right = vec![0.0f32; 32];
    provider.interpolate(direction(0.0, 0.0), 1.0, &mut left, &mut right);

    assert!(
        left.iter().all(|v| v.is_finite()) && right.iter().all(|v| v.is_finite()),
        "an interpolated HRTF must be finite"
    );
    assert!(
        left.iter().any(|v| *v != 0.0) && right.iter().any(|v| *v != 0.0),
        "an interpolated HRTF must not be silent"
    );
    // A source straight ahead reaches both ears at the same level.
    let diff: f32 = left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| (a - b).abs())
        .sum();
    assert!(
        diff < 1e-4,
        "a front source must reach both ears equally, the difference is {diff}"
    );
}

#[test]
fn an_off_grid_direction_still_produces_a_response() {
    // The grid is 30° × 30°, so 37° / 13° falls between four measured
    // directions. The renderer must interpolate rather than go silent.
    let dataset = synthetic_dataset();
    let provider = HrtfDatasetProvider::new(dataset);

    let mut left = vec![0.0f32; 32];
    let mut right = vec![0.0f32; 32];
    provider.interpolate(direction(37.0, 13.0), 1.0, &mut left, &mut right);

    assert!(
        left.iter().all(|v| v.is_finite()) && right.iter().all(|v| v.is_finite()),
        "an off-grid HRTF must be finite"
    );
    assert!(
        left.iter().any(|v| *v != 0.0) && right.iter().any(|v| *v != 0.0),
        "an off-grid direction must not be silent"
    );
}

#[test]
fn an_off_grid_response_is_a_blend_of_its_neighbours() {
    // The interpolation is bilinear in azimuth and elevation, so the value at
    // 15° (halfway between the 0° and 30° grid points) must be the average
    // of the two — that is what "interpolated" means, and it is checkable
    // without knowing anything about the filter.
    let dataset = synthetic_dataset();
    let az = dataset.azimuths().to_vec();
    assert!(
        az.windows(2)
            .any(|w| (w[0] - 0.0).abs() < 1e-6 && (w[1] - 30.0).abs() < 1e-6),
        "the synthetic grid must contain the 0° and 30° azimuths, it has {az:?}"
    );

    let grid_0 = dataset
        .ir(0, dataset.elevations().len() / 2, Ear::Left)
        .to_vec();
    let grid_30 = dataset
        .ir(1, dataset.elevations().len() / 2, Ear::Left)
        .to_vec();

    let mut mid = vec![0.0f32; dataset.taps()];
    dataset.bilinear_interpolate(15.0, 0.0, Ear::Left, &mut mid);

    for (i, m) in mid.iter().enumerate() {
        let expected = 0.5 * (grid_0[i] + grid_30[i]);
        assert!(
            (m - expected).abs() < 1e-6,
            "tap {i} of the half-way response is {m}, the average of its \
             neighbours is {expected}"
        );
    }
}

#[test]
fn the_dataset_reports_the_grid_it_was_built_on() {
    let dataset = synthetic_dataset();
    assert_eq!(dataset.taps(), 32);
    assert_eq!(
        dataset.azimuths().len(),
        12,
        "a 30° azimuth grid has 12 points"
    );
    assert!(dataset.elevations().len() >= 2);
    assert_eq!(
        dataset.measured_direction_count(),
        dataset.azimuths().len() * dataset.elevations().len()
    );
    let (az_min, az_max, el_min, el_max) = dataset
        .coverage_extents()
        .expect("a regular grid must report its coverage");
    assert!(az_min <= az_max);
    assert!(el_min <= el_max);
}

#[test]
fn the_analytic_head_model_reports_its_own_interaural_delay() {
    // A source on the right reaches the right ear first and the left ear
    // late — the whole point of the model.
    let radius = engine::spatial::hrtf::DEFAULT_HEAD_RADIUS;
    let speed = engine::spatial::hrtf::DEFAULT_SPEED_OF_SOUND;
    let right_azimuth = std::f32::consts::FRAC_PI_2;

    let right_ear = engine::spatial::ear_delay_sec(right_azimuth, Ear::Right, radius, speed);
    let left_ear = engine::spatial::ear_delay_sec(right_azimuth, Ear::Left, radius, speed);
    assert_eq!(right_ear, 0.0, "the near ear must not be delayed");
    assert!(
        left_ear > 0.0,
        "the far ear must be delayed, it is {left_ear} s"
    );

    let itd = engine::spatial::woodworth_itd_sec(right_azimuth, radius, speed);
    assert!(
        (left_ear - itd).abs() < 1e-9,
        "the far-ear delay must be the Woodworth ITD"
    );
    assert!(
        itd <= engine::spatial::max_itd_sec(radius, speed) + 1e-9,
        "the ITD must not exceed the documented maximum"
    );

    // The head shadow must attenuate the far ear more than the near one.
    let near_alpha = engine::spatial::head_shadow_alpha(right_azimuth, Ear::Right);
    let far_alpha = engine::spatial::head_shadow_alpha(right_azimuth, Ear::Left);
    assert!(
        near_alpha > far_alpha,
        "the far ear must be shadowed more (near {near_alpha}, far {far_alpha})"
    );
}

#[test]
fn the_renderer_reports_which_head_model_is_in_use() {
    // Without a dataset the renderer uses the analytic model, and it says
    // so: no taps, no grid.
    let mut renderer = BinauralRenderer::new(0.0);
    assert!(!renderer.has_dataset(), "no dataset must be loaded yet");
    assert_eq!(renderer.realized_hrtf_taps(), 0);
    assert!(
        renderer.hrtf_grid().is_none(),
        "an analytic renderer has no measured grid"
    );

    // With a dataset it says that too.
    let dataset = Arc::new(synthetic_dataset());
    renderer.use_dataset(Some(dataset));
    renderer
        .prepare(&SpeakerLayout::stereo(), SR)
        .expect("a stereo layout must be accepted by the binaural renderer");
    assert!(
        renderer.has_dataset(),
        "the dataset must be reported as loaded"
    );
    assert!(
        renderer.realized_hrtf_taps() > 0,
        "a spectral renderer must report the taps it convolves with"
    );
    assert!(
        renderer.hrtf_grid().is_some(),
        "a spectral renderer must report the grid it interpolates"
    );
}

#[test]
fn the_renderer_produces_finite_audible_output_for_an_off_grid_direction() {
    let mut renderer = BinauralRenderer::new(0.0);
    renderer
        .prepare(&SpeakerLayout::stereo(), SR)
        .expect("a stereo layout must be accepted by the binaural renderer");
    let out = render(&mut renderer, direction(37.0, 13.0));

    assert!(
        out.iter().all(|v| v.is_finite()),
        "the output must be finite"
    );
    let energy: f32 = out.iter().map(|v| v * v).sum();
    assert!(
        energy > 1e-6,
        "the renderer must produce audible output for an off-grid direction, \
         the energy was {energy:e}"
    );
}

#[test]
fn a_hard_off_source_reaches_the_two_ears_differently() {
    // A source hard to the right is shadowed and delayed at the left ear, so
    // the two ears must not receive identical signals. This is the audible
    // difference between a binaural renderer and a stereo pan.
    let mut renderer = BinauralRenderer::new(0.0);
    renderer
        .prepare(&SpeakerLayout::stereo(), SR)
        .expect("a stereo layout must be accepted by the binaural renderer");
    let out = render(&mut renderer, direction(90.0, 0.0));

    let frames = out.len() / 2;
    let mut max_difference = 0.0f32;
    for f in 0..frames {
        max_difference = max_difference.max((out[f * 2] - out[f * 2 + 1]).abs());
    }
    assert!(
        max_difference > 1e-3,
        "a hard-right source must reach the two ears differently, the largest \
         difference was {max_difference:e}"
    );
}

#[test]
fn a_non_stereo_layout_is_refused_with_a_typed_error() {
    // The binaural renderer *is* the head: it has exactly two ears, and a
    // 5.1 layout is a configuration error, not a silent downmix.
    let mut renderer = BinauralRenderer::new(0.0);
    let result = renderer.prepare(&SpeakerLayout::five_point_one(), SR);
    assert!(
        matches!(result, Err(engine::spatial::RenderError::InvalidLayout)),
        "a 5.1 layout must be refused, got {result:?}"
    );
}
