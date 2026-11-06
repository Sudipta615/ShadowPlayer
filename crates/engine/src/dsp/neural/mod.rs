//! Machine Learning & Neural Audio Inference Engine.
//!
//! # Overview
//!
//! Ultralightweight, zero-allocation real-time neural inference engine designed
//! for audiophile signal enhancement and analog saturation emulation:
//! - **Dense & GRU Layers**: Vectorized forward pass with zero allocations.
//! - **Hardware Emulation**: 12AX7 triode tube preamp and Studer A800 tape saturation.
//! - **Neural De-Clipping**: Smooth restoration of clipped peaks on hot masters.
//! - **RTNeural / JSON Interop**: Flexible import for custom-trained models.
//!
//! # Realtime Contract
//!
//! Layer evaluations run in-place across pre-allocated workspace buffers.
//! Zero heap allocations, zero locks, zero blocking I/O on the audio thread.

pub mod declipper;
pub mod layers;
pub mod model;
pub mod processor;

pub use declipper::NeuralDeclipper;
pub use layers::{ActivationType, DenseLayer, GruLayer};
pub use model::{NeuralLayer, NeuralModel};
pub use processor::NeuralProcessor;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dense_layer_forward() {
        let l = DenseLayer::new(
            2,
            2,
            vec![1.0, 0.0, 0.0, 2.0],
            vec![0.5, -0.5],
            ActivationType::Linear,
        )
        .expect("valid layer");

        let input = [1.0, 2.0];
        let mut output = [0.0; 2];
        l.forward(&input, &mut output);

        assert!((output[0] - 1.5).abs() < 1e-6);
        assert!((output[1] - 3.5).abs() < 1e-6);
    }

    #[test]
    fn test_dense_layer_activations() {
        let l_relu = DenseLayer::new(1, 1, vec![1.0], vec![-0.5], ActivationType::ReLU)
            .expect("valid layer");
        let mut out = [0.0];
        l_relu.forward(&[0.2], &mut out);
        assert_eq!(out[0], 0.0);
        l_relu.forward(&[1.0], &mut out);
        assert!((out[0] - 0.5).abs() < 1e-6);

        let l_tanh =
            DenseLayer::new(1, 1, vec![1.0], vec![0.0], ActivationType::Tanh).expect("valid layer");
        l_tanh.forward(&[0.0], &mut out);
        assert_eq!(out[0], 0.0);
        l_tanh.forward(&[1.0], &mut out);
        assert!((out[0] - 1.0_f32.tanh()).abs() < 1e-6);
    }

    #[test]
    fn test_gru_layer_recurrent_step() {
        let in_f = 1;
        let hidden = 2;
        let w_ih = vec![0.5; 3 * hidden * in_f];
        let w_hh = vec![0.1; 3 * hidden * hidden];
        let b_ih = vec![0.0; 3 * hidden];
        let b_hh = vec![0.0; 3 * hidden];

        let mut gru = GruLayer::new(in_f, hidden, w_ih, w_hh, b_ih, b_hh).expect("valid gru");
        let mut out = [0.0; 2];

        gru.forward_step(&[0.5], &mut out);
        assert!(out[0].is_finite() && out[1].is_finite());
        assert!(out[0] != 0.0 || out[1] != 0.0);

        gru.reset();
        assert_eq!(gru.hidden_state, vec![0.0; 2]);
    }

    #[test]
    fn test_tube_preamp_factory_model() {
        let mut model = NeuralModel::tube_preamp_12ax7();
        assert_eq!(model.in_features, 1);
        assert_eq!(model.out_features, 1);

        let mut out = [0.0];
        model.forward(&[0.0], &mut out);
        // Tube model generates slight positive asymmetry at DC/silence (cathode bias)
        assert!(out[0].is_finite());

        model.forward(&[1.0], &mut out);
        assert!(out[0].is_finite() && out[0].abs() < 1.5);
    }

    #[test]
    fn test_tape_saturation_factory_model() {
        let mut model = NeuralModel::tape_saturation_studer();
        let mut out = [0.0];

        for i in 0..100 {
            let x = (i as f32 * 0.1).sin();
            model.forward(&[x], &mut out);
            assert!(out[0].is_finite());
            assert!(out[0].abs() <= 1.0);
        }
    }

    #[test]
    fn test_neural_processor_stereo_block() {
        let mut proc = NeuralProcessor::tube_preamp();
        proc.set_enabled(true);
        proc.set_drive_db(6.0);
        proc.set_wet_mix(0.8);

        let mut left = vec![0.3; 256];
        let mut right = vec![-0.3; 256];

        proc.process_block(&mut left, &mut right);

        for &s in &left {
            assert!(s.is_finite());
        }
        for &s in &right {
            assert!(s.is_finite());
        }
    }

    #[test]
    fn test_neural_declipper() {
        let mut declipper = NeuralDeclipper::new(0.95);
        declipper.set_enabled(true);

        // A normal unclipped sample
        let (l, r) = declipper.process_sample(0.5, 0.5);
        assert_eq!(l, 0.5);
        assert_eq!(r, 0.5);

        // A clipped sample above threshold
        let (cl, cr) = declipper.process_sample(0.99, 0.99);
        assert!(cl > 0.95 && cl.is_finite());
        assert!(cr > 0.95 && cr.is_finite());
    }

    #[test]
    fn test_model_json_roundtrip() {
        let model = NeuralModel::tube_preamp_12ax7();
        let json = serde_json::to_string(&model).expect("serialize");
        let mut loaded = NeuralModel::from_json(&json).expect("deserialize");

        let mut out_orig = [0.0];
        let mut out_loaded = [0.0];
        let mut m_orig = model;
        m_orig.forward(&[0.5], &mut out_orig);
        loaded.forward(&[0.5], &mut out_loaded);

        assert!((out_orig[0] - out_loaded[0]).abs() < 1e-6);
    }
}
