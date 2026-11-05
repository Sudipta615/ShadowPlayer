//! Named table layouts: built-in presets and the user's own.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::super::prefs::ColumnWidths;
use super::SongColumn;

/// A named table layout: an ordered list of columns and their width shares.
///
/// Presets are data, not code: the built-in Default / Casual / Audiophile
/// layouts can be edited, copied or replaced, and the user's own presets are
/// stored with the rest of the table preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnPreset {
    pub name: String,
    pub columns: Vec<SongColumn>,
    /// One share per column, parallel to `columns`.
    pub weights: Vec<f32>,
}

impl Default for ColumnPreset {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            columns: vec![SongColumn::Title, SongColumn::Artist, SongColumn::Album],
            weights: vec![0.42, 0.26, 0.26],
        }
        .normalized()
    }
}

impl ColumnPreset {
    /// Build the Default layout while keeping the column proportions an older
    /// build saved in `SongTablePrefs::widths`.
    pub fn from_legacy_widths(widths: ColumnWidths) -> Self {
        let widths = widths.normalized();
        Self {
            name: "Default".to_string(),
            columns: vec![SongColumn::Title, SongColumn::Artist, SongColumn::Album],
            weights: vec![widths.title, widths.artist, widths.album],
        }
        .normalized()
    }

    /// The factory layout with this name, if it is one of the built-ins.
    ///
    /// "Reset this preset" uses this to restore Casual or Audiophile to the
    /// layout they shipped with, rather than collapsing every preset to the
    /// Default columns.
    pub fn factory(name: &str) -> Option<Self> {
        Self::builtins()
            .into_iter()
            .find(|preset| preset.name.eq_ignore_ascii_case(name))
    }

    /// The presets a fresh install starts with.
    pub fn builtins() -> Vec<Self> {
        vec![
            Self::default(),
            Self {
                name: "Casual".to_string(),
                columns: vec![SongColumn::Title, SongColumn::Artist],
                weights: vec![0.68, 0.32],
            }
            .normalized(),
            Self {
                name: "Audiophile".to_string(),
                columns: vec![
                    SongColumn::Title,
                    SongColumn::Artist,
                    SongColumn::Album,
                    SongColumn::Codec,
                    SongColumn::BitDepth,
                    SongColumn::SampleRate,
                    SongColumn::Bpm,
                ],
                weights: vec![0.30, 0.17, 0.17, 0.09, 0.08, 0.10, 0.09],
            }
            .normalized(),
        ]
    }

    /// Title first, no duplicates, one valid weight per column.
    pub fn normalized(mut self) -> Self {
        let mut seen = HashSet::new();
        self.columns.retain(|column| seen.insert(*column));
        if let Some(pos) = self
            .columns
            .iter()
            .position(|column| *column == SongColumn::Title)
        {
            let title = self.columns.remove(pos);
            self.columns.insert(0, title);
        } else {
            self.columns.insert(0, SongColumn::Title);
        }

        let mut weights = Vec::with_capacity(self.columns.len());
        for (idx, column) in self.columns.iter().enumerate() {
            let weight = self
                .weights
                .get(idx)
                .copied()
                .unwrap_or(column.default_weight());
            weights.push(if weight.is_finite() {
                weight.max(0.0)
            } else {
                column.default_weight()
            });
        }
        let sum: f32 = weights.iter().sum();
        if !sum.is_finite() || sum <= f32::EPSILON {
            weights = self
                .columns
                .iter()
                .map(|column| column.default_weight())
                .collect();
        } else {
            for weight in &mut weights {
                *weight /= sum;
            }
        }
        self.weights = Self::enforce_minimums(&weights);
        if self.name.trim().is_empty() {
            self.name = "Custom".to_string();
        }
        self
    }

    fn enforce_minimums(weights: &[f32]) -> Vec<f32> {
        const MIN: f32 = 0.06;
        let mut out = weights.to_vec();
        if out.len() <= 1 {
            return vec![1.0];
        }
        let min = MIN.min(1.0 / out.len() as f32);
        let mut deficit = 0.0;
        for weight in &mut out {
            if *weight < min {
                deficit += min - *weight;
                *weight = min;
            }
        }
        if deficit > 0.0 {
            let headroom: f32 = out.iter().map(|weight| (weight - min).max(0.0)).sum();
            if headroom <= f32::EPSILON {
                return vec![1.0 / out.len() as f32; out.len()];
            }
            for weight in &mut out {
                if *weight > min {
                    *weight -= deficit * (*weight - min) / headroom;
                }
            }
        }
        let sum: f32 = out.iter().sum();
        for weight in &mut out {
            *weight /= sum;
        }
        out
    }

    /// Drag the boundary after column `boundary` by `delta` fractions of the
    /// flexible width.
    pub fn drag(&self, boundary: usize, delta: f32) -> Self {
        let mut next = self.clone().normalized();
        if boundary + 1 >= next.columns.len() {
            return next;
        }
        const MIN: f32 = 0.06;
        let (left, right) = next.weights.split_at_mut(boundary + 1);
        let left = &mut left[boundary];
        let right = &mut right[0];
        let delta = delta.clamp(MIN - *left, *right - MIN);
        *left += delta;
        *right -= delta;
        let sum: f32 = next.weights.iter().sum();
        if sum > f32::EPSILON {
            for weight in &mut next.weights {
                *weight /= sum;
            }
        }
        next
    }

    pub fn move_column(&mut self, from: usize, to: usize) {
        if from < self.columns.len() && to < self.columns.len() {
            let column = self.columns.remove(from);
            let weight = self.weights.remove(from);
            self.columns.insert(to, column);
            self.weights.insert(to, weight);
        }
    }
}
