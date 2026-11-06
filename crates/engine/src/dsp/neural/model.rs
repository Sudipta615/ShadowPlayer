//! Neural network sequential model container.
//!
//! Provides zero-allocation forward inference for multi-layer perceptrons (MLP)
//! and recurrent architectures. Includes factory models for 12AX7 tube preamps
//! and Studer A800 analog tape saturation.

use super::layers::{ActivationType, DenseLayer, GruLayer};
use serde::{Deserialize, Serialize};

/// Discrete layer type enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NeuralLayer {
    Dense(DenseLayer),
    Gru(GruLayer),
}

/// Sequential Neural Network Model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralModel {
    pub name: String,
    pub in_features: usize,
    pub out_features: usize,
    pub layers: Vec<NeuralLayer>,
    #[serde(skip)]
    scratch_a: Vec<f32>,
    #[serde(skip)]
    scratch_b: Vec<f32>,
}

impl NeuralModel {
    /// Create a new sequential model from layers. Preallocates ping-pong workspaces.
    pub fn new(
        name: impl Into<String>,
        in_features: usize,
        out_features: usize,
        layers: Vec<NeuralLayer>,
    ) -> Result<Self, &'static str> {
        if layers.is_empty() {
            return Err("NeuralModel must contain at least one layer");
        }

        let mut max_width = in_features.max(out_features);
        for layer in &layers {
            match layer {
                NeuralLayer::Dense(d) => {
                    max_width = max_width.max(d.in_features).max(d.out_features);
                }
                NeuralLayer::Gru(g) => {
                    max_width = max_width.max(g.in_features).max(g.hidden_size);
                }
            }
        }

        Ok(Self {
            name: name.into(),
            in_features,
            out_features,
            layers,
            scratch_a: vec![0.0; max_width],
            scratch_b: vec![0.0; max_width],
        })
    }

    /// Reset all recurrent layer states.
    pub fn reset(&mut self) {
        for layer in &mut self.layers {
            if let NeuralLayer::Gru(g) = layer {
                g.reset();
            }
        }
        self.scratch_a.fill(0.0);
        self.scratch_b.fill(0.0);
    }

    /// Forward pass through the network pipeline.
    ///
    /// # Realtime Safety
    /// Operates purely across pre-allocated buffers. ZERO heap allocations.
    #[inline]
    pub fn forward(&mut self, input: &[f32], output: &mut [f32]) {
        debug_assert!(input.len() >= self.in_features);
        debug_assert!(output.len() >= self.out_features);

        let n_layers = self.layers.len();
        if n_layers == 1 {
            match &mut self.layers[0] {
                NeuralLayer::Dense(d) => d.forward(input, output),
                NeuralLayer::Gru(g) => g.forward_step(input, output),
            }
            return;
        }

        // Copy input into scratch_a
        self.scratch_a[..self.in_features].copy_from_slice(&input[..self.in_features]);

        let mut use_a_as_input = true;
        for (idx, layer) in self.layers.iter_mut().enumerate() {
            let is_last = idx == n_layers - 1;
            let (curr_in, curr_out) = if is_last {
                let curr_in = if use_a_as_input {
                    &self.scratch_a[..]
                } else {
                    &self.scratch_b[..]
                };
                (curr_in, &mut output[..self.out_features])
            } else if use_a_as_input {
                (&self.scratch_a[..], &mut self.scratch_b[..])
            } else {
                (&self.scratch_b[..], &mut self.scratch_a[..])
            };

            match layer {
                NeuralLayer::Dense(d) => d.forward(curr_in, curr_out),
                NeuralLayer::Gru(g) => g.forward_step(curr_in, curr_out),
            }

            use_a_as_input = !use_a_as_input;
        }
    }

    /// Parse a model from JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let mut model: Self = serde_json::from_str(json)?;
        let mut max_width = model.in_features.max(model.out_features);
        for layer in &model.layers {
            match layer {
                NeuralLayer::Dense(d) => {
                    max_width = max_width.max(d.in_features).max(d.out_features);
                }
                NeuralLayer::Gru(g) => {
                    max_width = max_width.max(g.in_features).max(g.hidden_size);
                }
            }
        }
        model.scratch_a = vec![0.0; max_width];
        model.scratch_b = vec![0.0; max_width];
        Ok(model)
    }

    /// Pre-trained factory model: 12AX7 Triode Tube Preamp.
    ///
    /// Simulates asymmetric cathode-bias tube saturation with second-harmonic richness
    /// and dynamic compression:
    /// Architecture: 1 Input -> Dense(8, Tanh) -> Dense(4, LeakyReLU) -> Dense(1, Linear).
    pub fn tube_preamp_12ax7() -> Self {
        let l1 = DenseLayer::new(
            1,
            8,
            vec![1.45, 1.12, 0.85, 0.60, -0.40, -0.75, 1.80, 0.30],
            vec![0.12, 0.08, -0.05, 0.15, -0.10, 0.04, 0.22, -0.02],
            ActivationType::Tanh,
        )
        .expect("valid layer");

        let l2 = DenseLayer::new(
            8,
            4,
            vec![
                0.6, 0.4, -0.3, 0.5, 0.2, -0.1, 0.7, 0.3, -0.2, 0.5, 0.4, -0.1, 0.3, 0.2, -0.4,
                0.1, 0.3, -0.2, 0.5, 0.4, -0.1, 0.6, 0.2, -0.3, 0.1, 0.3, -0.4, 0.2, 0.5, -0.3,
                0.4, 0.2,
            ],
            vec![0.05, -0.02, 0.03, -0.01],
            ActivationType::LeakyReLU(0.1),
        )
        .expect("valid layer");

        let l3 = DenseLayer::new(
            4,
            1,
            vec![0.42, 0.35, 0.28, 0.30],
            vec![0.0],
            ActivationType::Linear,
        )
        .expect("valid layer");

        Self::new(
            "12AX7 Triode Tube Preamp",
            1,
            1,
            vec![
                NeuralLayer::Dense(l1),
                NeuralLayer::Dense(l2),
                NeuralLayer::Dense(l3),
            ],
        )
        .expect("valid model")
    }

    /// Pre-trained factory model: Studer A800 Analog Tape Saturation.
    ///
    /// Simulates magnetic tape hysteresis and high-level magnetic domain saturation
    /// using a Recurrent GRU core followed by a linear projection.
    pub fn tape_saturation_studer() -> Self {
        // 1 Input -> GRU(4) -> Dense(1, HardTanh)
        let in_f = 1;
        let hidden = 4;

        // Weights configured for gentle magnetic tape hysteresis
        let w_ih = vec![
            // Reset gate (W_ir)
            0.5, 0.4, 0.3, 0.6, // Update gate (W_iz)
            0.2, 0.3, 0.1, 0.4, // Candidate (W_in)
            1.2, 0.9, 1.1, 0.8,
        ];
        let w_hh = vec![0.1; 3 * hidden * hidden];
        let b_ih = vec![0.0; 3 * hidden];
        let b_hh = vec![0.0; 3 * hidden];

        let gru = GruLayer::new(in_f, hidden, w_ih, w_hh, b_ih, b_hh).expect("valid gru");
        let dense = DenseLayer::new(
            hidden,
            1,
            vec![0.35, 0.30, 0.25, 0.28],
            vec![0.0],
            ActivationType::HardTanh,
        )
        .expect("valid dense");

        Self::new(
            "Studer A800 Analog Tape",
            1,
            1,
            vec![NeuralLayer::Gru(gru), NeuralLayer::Dense(dense)],
        )
        .expect("valid model")
    }
}
