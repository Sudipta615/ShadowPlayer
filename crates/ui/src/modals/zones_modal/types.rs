//! State definitions for the Zone Manager modal.

use domain::ids::ZoneId;
use domain::smart::SmartOp;
use domain::zone::ZoneMatchField;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZoneModalTab {
    #[default]
    Zones,
    RoutingRules,
}

#[derive(Debug, Clone)]
pub struct ZoneManagerModalState {
    pub is_open: bool,
    pub active_tab: ZoneModalTab,
    pub selected_zone_id: Option<ZoneId>,
    pub new_zone_name: String,
    pub new_zone_device: String,
    pub rule_name: String,
    pub rule_target_zone: Option<ZoneId>,
    pub rule_field: ZoneMatchField,
    pub rule_op: SmartOp,
    pub rule_val: String,
    pub rule_priority: i32,
}

impl Default for ZoneManagerModalState {
    fn default() -> Self {
        Self {
            is_open: false,
            active_tab: ZoneModalTab::Zones,
            selected_zone_id: None,
            new_zone_name: String::new(),
            new_zone_device: String::new(),
            rule_name: String::new(),
            rule_target_zone: None,
            rule_field: ZoneMatchField::Format,
            rule_op: SmartOp::Equals,
            rule_val: "DSD".to_string(),
            rule_priority: 10,
        }
    }
}
