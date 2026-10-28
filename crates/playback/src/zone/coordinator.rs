//! Multi-zone coordinator managing active zones, master/slave links, and ZoneSwitch routing.

use std::collections::HashMap;

use domain::ids::ZoneId;
use domain::models::Track;
use domain::zone::{ZoneConfig, ZoneSwitchRule};

use super::types::ZonePlaybackState;

/// Multi-zone audio coordinator orchestrating concurrent playback states across zones.
pub struct ZoneCoordinator {
    pub active_zone_id: ZoneId,
    pub zones: HashMap<ZoneId, ZonePlaybackState>,
    pub rules: Vec<ZoneSwitchRule>,
}

impl ZoneCoordinator {
    /// Construct a new coordinator with an initial default primary zone.
    pub fn new(default_zone: ZoneConfig) -> Self {
        let active_zone_id = default_zone.id;
        let mut zones = HashMap::new();
        zones.insert(active_zone_id, ZonePlaybackState::new(default_zone));

        Self {
            active_zone_id,
            zones,
            rules: Vec::new(),
        }
    }

    /// Restore coordinator from stored database configurations and rules.
    pub fn from_stored(configs: Vec<ZoneConfig>, rules: Vec<ZoneSwitchRule>) -> Self {
        let mut zones = HashMap::new();
        let mut default_id = None;

        for cfg in configs {
            if cfg.is_default && default_id.is_none() {
                default_id = Some(cfg.id);
            }
            zones.insert(cfg.id, ZonePlaybackState::new(cfg));
        }

        let active_zone_id = default_id
            .or_else(|| zones.keys().copied().next())
            .unwrap_or_else(ZoneId::new);

        Self {
            active_zone_id,
            zones,
            rules,
        }
    }

    /// Return the currently focused/active zone ID.
    pub fn active_zone_id(&self) -> ZoneId {
        self.active_zone_id
    }

    /// Change the active focused zone.
    pub fn set_active_zone(&mut self, id: ZoneId) {
        if self.zones.contains_key(&id) {
            self.active_zone_id = id;
        }
    }

    /// Borrow the currently focused zone's playback state.
    pub fn active_zone(&self) -> Option<&ZonePlaybackState> {
        self.zones.get(&self.active_zone_id)
    }

    /// Mutably borrow the currently focused zone's playback state.
    pub fn active_zone_mut(&mut self) -> Option<&mut ZonePlaybackState> {
        self.zones.get_mut(&self.active_zone_id)
    }

    /// Look up an arbitrary zone by id.
    pub fn get_zone(&self, id: ZoneId) -> Option<&ZonePlaybackState> {
        self.zones.get(&id)
    }

    /// Mutably look up an arbitrary zone by id.
    pub fn get_zone_mut(&mut self, id: ZoneId) -> Option<&mut ZonePlaybackState> {
        self.zones.get_mut(&id)
    }

    /// Return all zones as a sorted slice of references (default zone first, then alphabetical).
    pub fn all_zones(&self) -> Vec<&ZonePlaybackState> {
        let mut list: Vec<&ZonePlaybackState> = self.zones.values().collect();
        list.sort_by(|a, b| {
            b.config.is_default.cmp(&a.config.is_default).then_with(|| {
                a.config
                    .name
                    .to_lowercase()
                    .cmp(&b.config.name.to_lowercase())
            })
        });
        list
    }

    /// Register a new audio zone.
    pub fn add_zone(&mut self, config: ZoneConfig) {
        let id = config.id;
        self.zones.insert(id, ZonePlaybackState::new(config));
    }

    /// Remove a zone. Protected: cannot remove the default primary zone.
    /// If removing the currently active zone, shifts focus to the default zone.
    pub fn remove_zone(&mut self, id: ZoneId) -> bool {
        if let Some(zone) = self.zones.get(&id) {
            if zone.config.is_default {
                return false;
            }
        } else {
            return false;
        }

        self.zones.remove(&id);

        // Clear any links pointing to this zone
        for z in self.zones.values_mut() {
            if z.config.linked_master_id == Some(id) {
                z.config.linked_master_id = None;
            }
        }

        if self.active_zone_id == id {
            if let Some(default_zone) = self.zones.values().find(|z| z.config.is_default) {
                self.active_zone_id = default_zone.id;
            } else if let Some(first_id) = self.zones.keys().copied().next() {
                self.active_zone_id = first_id;
            }
        }

        true
    }

    /// Link a slave zone to a master zone with a given relative timing offset (milliseconds).
    pub fn link_zones(&mut self, master: ZoneId, slave: ZoneId, timing_offset_ms: f32) {
        if master == slave || !self.zones.contains_key(&master) {
            return;
        }

        if let Some(slave_state) = self.zones.get_mut(&slave) {
            slave_state.config.linked_master_id = Some(master);
            slave_state.config.set_timing_offset(timing_offset_ms);
        }
    }

    /// Unlink a slave zone so it plays autonomously.
    pub fn unlink_zone(&mut self, slave: ZoneId) {
        if let Some(slave_state) = self.zones.get_mut(&slave) {
            slave_state.config.linked_master_id = None;
        }
    }

    /// Query all slave zones currently linked to a given master zone.
    pub fn get_linked_slaves(&self, master: ZoneId) -> Vec<ZoneId> {
        self.zones
            .values()
            .filter(|z| z.config.linked_master_id == Some(master))
            .map(|z| z.id)
            .collect()
    }

    /// Synchronize playback state across all slave zones linked to a given master zone.
    ///
    /// Applies each slave's relative `timing_offset_ms` to avoid acoustic comb filtering
    /// and echo across adjacent rooms.
    pub fn sync_linked_playback(
        &mut self,
        master_id: ZoneId,
        is_playing: bool,
        position_secs: f32,
    ) {
        let slaves = self.get_linked_slaves(master_id);
        for slave_id in slaves {
            if let Some(slave) = self.zones.get_mut(&slave_id) {
                slave.is_playing = is_playing;
                let offset_secs = slave.config.timing_offset_ms / 1000.0;
                let adjusted_pos = (position_secs + offset_secs).max(0.0);
                slave.seek(adjusted_pos);
            }
        }
    }

    /// Update dynamic clock drift compensation for a slave zone based on measured buffer delay.
    pub fn compensate_clock_drift(&mut self, slave_id: ZoneId, phase_error_ms: f32) -> f32 {
        if let Some(slave) = self.zones.get_mut(&slave_id) {
            let drift = slave.clock_drift.update_from_phase_error(phase_error_ms);
            slave.config.set_clock_drift(drift);
            drift
        } else {
            0.0
        }
    }

    /// Evaluate ZoneSwitch rules for a track.
    ///
    /// Iterates rules in descending priority order. Returns the first matching target zone.
    pub fn evaluate_zone_switch(&self, track: &Track) -> Option<ZoneId> {
        for rule in &self.rules {
            if rule.enabled && self.zones.contains_key(&rule.target_zone_id) && rule.matches(track)
            {
                return Some(rule.target_zone_id);
            }
        }
        None
    }

    /// Replace active routing rules with a new set.
    pub fn set_rules(&mut self, mut rules: Vec<ZoneSwitchRule>) {
        rules.sort_by_key(|a| std::cmp::Reverse(a.priority));
        self.rules = rules;
    }

    /// Borrow active routing rules.
    pub fn rules(&self) -> &[ZoneSwitchRule] {
        &self.rules
    }
}
