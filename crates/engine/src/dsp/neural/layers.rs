//! Neural network layer primitives for real-time audio inference.
//!
//! Designed for zero-allocation execution on the audio thread. All workspaces
//! and layer dimensions are pre-allocated during construction.

use serde::{Deserialize, Serialize};

/// Supported activation functions for neural audio modeling.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ActivationType {
    Linear,
    Tanh,
    Sigmoid,
    ReLU,
    LeakyReLU(f32),
    HardTanh,
}

impl ActivationType {
    /// Apply the activation function element-wise.
    #[inline(always)]
    pub fn apply(&self, x: f32) -> f32 {
        match self {
            Self::Linear => x,
            Self::Tanh => x.tanh(),
            Self::Sigmoid => 1.0 / (1.0 + (-x).exp()),
            Self::ReLU => x.max(0.0),
            Self::LeakyReLU(alpha) => {
                if x > 0.0 {
                    x
                } else {
                    *alpha * x
                }
            }
            Self::HardTanh => x.clamp(-1.0, 1.0),
        }
    }
}

/// Fully connected / dense feedforward layer: `y = act(W * x + b)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenseLayer {
    pub in_features: usize,
    pub out_features: usize,
    /// Row-major weights with shape `[out_features, in_features]`.
    pub weights: Vec<f32>,
    /// Bias vector with shape `[out_features]`.
    pub bias: Vec<f32>,
    pub activation: ActivationType,
}

impl DenseLayer {
    /// Create a new dense layer with specified dimensions and weights.
    pub fn new(
        in_features: usize,
        out_features: usize,
        weights: Vec<f32>,
        bias: Vec<f32>,
        activation: ActivationType,
    ) -> Result<Self, &'static str> {
        if weights.len() != in_features * out_features {
            return Err("DenseLayer: weight matrix dimension mismatch");
        }
        if bias.len() != out_features {
            return Err("DenseLayer: bias vector dimension mismatch");
        }
        Ok(Self {
            in_features,
            out_features,
            weights,
            bias,
            activation,
        })
    }

    /// Forward pass executing in-place into a pre-allocated output buffer.
    ///
    /// # Safety & Realtime Guarantees
    /// This function performs ZERO heap allocations and ZERO locks.
    #[inline]
    pub fn forward(&self, input: &[f32], output: &mut [f32]) {
        debug_assert!(input.len() >= self.in_features);
        debug_assert!(output.len() >= self.out_features);

        let in_len = self.in_features;
        for (o, out_val) in output.iter_mut().take(self.out_features).enumerate() {
            let row_start = o * in_len;
            let row = &self.weights[row_start..row_start + in_len];
            let mut acc = self.bias[o];
            for (i, &in_val) in input.iter().take(in_len).enumerate() {
                acc += row[i] * in_val;
            }
            *out_val = self.activation.apply(acc);
        }
    }
}

/// Gated Recurrent Unit (GRU) layer for temporal modeling and hysteresis.
///
/// Implements standard PyTorch/RTNeural compatible GRU formulation:
/// ```text
/// r_t = sigmoid(W_ir * x_t + b_ir + W_hr * h_{t-1} + b_hr)
/// z_t = sigmoid(W_iz * x_t + b_iz + W_hz * h_{t-1} + b_hz)
/// n_t = tanh(W_in * x_t + b_in + r_t * (W_hn * h_{t-1} + b_hn))
/// h_t = (1 - z_t) * n_t + z_t * h_{t-1}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GruLayer {
    pub in_features: usize,
    pub hidden_size: usize,
    /// Stacked input weights `[W_ir; W_iz; W_in]`, length `3 * hidden_size * in_features`.
    pub weight_ih: Vec<f32>,
    /// Stacked hidden weights `[W_hr; W_hz; W_hn]`, length `3 * hidden_size * hidden_size`.
    pub weight_hh: Vec<f32>,
    /// Stacked input bias `[b_ir; b_iz; b_in]`, length `3 * hidden_size`.
    pub bias_ih: Vec<f32>,
    /// Stacked hidden bias `[b_hr; b_hz; b_hn]`, length `3 * hidden_size`.
    pub bias_hh: Vec<f32>,
    /// Running hidden state memory `[hidden_size]`.
    pub hidden_state: Vec<f32>,
    /// Pre-allocated scratch workspace for gate calculations.
    #[serde(skip)]
    workspace_gates: Vec<f32>,
}

impl GruLayer {
    /// Construct a new GRU layer with allocated weights and hidden state.
    pub fn new(
        in_features: usize,
        hidden_size: usize,
        weight_ih: Vec<f32>,
        weight_hh: Vec<f32>,
        bias_ih: Vec<f32>,
        bias_hh: Vec<f32>,
    ) -> Result<Self, &'static str> {
        let expected_ih = 3 * hidden_size * in_features;
        let expected_hh = 3 * hidden_size * hidden_size;
        let expected_b = 3 * hidden_size;

        if weight_ih.len() != expected_ih
            || weight_hh.len() != expected_hh
            || bias_ih.len() != expected_b
            || bias_hh.len() != expected_b
        {
            return Err("GruLayer: weight or bias dimension mismatch");
        }

        Ok(Self {
            in_features,
            hidden_size,
            weight_ih,
            weight_hh,
            bias_ih,
            bias_hh,
            hidden_state: vec![0.0; hidden_size],
            workspace_gates: vec![0.0; 3 * hidden_size],
        })
    }

    /// Reset internal recurrent hidden state to silence.
    pub fn reset(&mut self) {
        self.hidden_state.fill(0.0);
        self.workspace_gates.fill(0.0);
    }

    /// Process a single time-step, updating hidden state and writing output.
    ///
    /// # Safety & Realtime Guarantees
    /// Pure in-place operations with ZERO allocations.
    #[inline]
    pub fn forward_step(&mut self, input: &[f32], output: &mut [f32]) {
        let h_dim = self.hidden_size;
        let in_dim = self.in_features;

        debug_assert!(input.len() >= in_dim);
        debug_assert!(output.len() >= h_dim);

        if self.workspace_gates.len() < 3 * h_dim {
            self.workspace_gates.resize(3 * h_dim, 0.0);
        }

        // 1. Compute gate pre-activations: W_ih * x + b_ih + b_hh
        for g in 0..3 * h_dim {
            let row_start = g * in_dim;
            let mut acc = self.bias_ih[g] + self.bias_hh[g];
            for (i, &in_val) in input.iter().take(in_dim).enumerate() {
                acc += self.weight_ih[row_start + i] * in_val;
            }
            self.workspace_gates[g] = acc;
        }

        // 2. Add recurrent contributions for reset gate (r) and update gate (z)
        // W_hr * h and W_hz * h
        for g in 0..2 * h_dim {
            let row_start = g * h_dim;
            let mut acc = self.workspace_gates[g];
            for (j, &h_val) in self.hidden_state.iter().take(h_dim).enumerate() {
                acc += self.weight_hh[row_start + j] * h_val;
            }
            // Sigmoid activation
            self.workspace_gates[g] = 1.0 / (1.0 + (-acc).exp());
        }

        // 3. Compute candidate new state n_t:
        // n_pre = (W_in * x + b_in + b_hn) + r_t * (W_hn * h)
        let n_offset = 2 * h_dim;
        for (j, out_val) in output.iter_mut().take(h_dim).enumerate() {
            let row_start = (n_offset + j) * h_dim;
            let mut h_acc = 0.0f32;
            for (k, &h_val) in self.hidden_state.iter().take(h_dim).enumerate() {
                h_acc += self.weight_hh[row_start + k] * h_val;
            }
            let r_j = self.workspace_gates[j]; // Reset gate r_t
            let n_pre = self.workspace_gates[n_offset + j] + r_j * h_acc;
            let n_t = n_pre.tanh();

            // 4. Update hidden state: h_t = (1 - z_t) * n_t + z_t * h_{t-1}
            let z_j = self.workspace_gates[h_dim + j]; // Update gate z_t
            let new_h = (1.0 - z_j) * n_t + z_j * self.hidden_state[j];
            let clean_h = crate::buffer::flush_denormal(new_h);
            self.hidden_state[j] = clean_h;
            *out_val = clean_h;
        }
    }
}
