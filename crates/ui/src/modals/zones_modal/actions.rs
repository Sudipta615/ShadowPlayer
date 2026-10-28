//! Typed actions emitted by the Zone Manager modal.

use domain::ids::ZoneId;
use domain::zone::ZoneSwitchRule;

#[derive(Debug, Clone)]
pub enum ZoneManagerAction {
    SetActiveZone(ZoneId),
    CreateZone {
        name: String,
        device: Option<String>,
    },
    DeleteZone(ZoneId),
    SetOutputDevice {
        zone_id: ZoneId,
        device: Option<String>,
    },
    SetVolume {
        zone_id: ZoneId,
        volume: f32,
    },
    SetMuted {
        zone_id: ZoneId,
        muted: bool,
    },
    LinkZone {
        master: ZoneId,
        slave: ZoneId,
        offset_ms: f32,
    },
    UnlinkZone(ZoneId),
    SetTimingOffset {
        zone_id: ZoneId,
        offset_ms: f32,
    },
    SetClockDrift {
        zone_id: ZoneId,
        drift_pct: f32,
    },
    SaveRule(ZoneSwitchRule),
    DeleteRule(String),
    ToggleRule {
        rule_id: String,
        enabled: bool,
    },
}
