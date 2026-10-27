//! Display topology and physical zone mapping.
//!
//! Provides a zone model ([`Zone`], [`ZoneType`]) and topology mapping ([`DisplayTopology`], [`MappedDisplay`])
//! to translate raw network/system display addresses (e.g. `192.168.1.45`, `HDMI-1`) into
//! semantic location contexts like "the entrance display in the restaurant".

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::ProbeInfo;

/// Standard zone category classification for display deployments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneType {
    /// Entrance / foyer / reception display.
    Entrance,
    /// Point of sale / checkout / cashier display.
    Checkout,
    /// Waiting room / customer lobby area.
    WaitingArea,
    /// Store aisle or product section (e.g. "aisle_3").
    Aisle(String),
    /// Kitchen order display system (KDS) or food preparation area.
    Kitchen,
    /// Loading dock / warehouse shipping & receiving.
    LoadingDock,
    /// Conference / meeting / briefing room.
    MeetingRoom,
    /// Service counter / bar / pickup desk.
    Counter,
    /// Feature wall / video wall screen.
    Wall,
    /// Custom / user-defined zone category.
    Custom(String),
}

impl Default for ZoneType {
    fn default() -> Self {
        ZoneType::Custom("general".to_string())
    }
}

impl ZoneType {
    /// Identifier string representation of the zone category.
    pub fn as_str(&self) -> &str {
        match self {
            ZoneType::Entrance => "entrance",
            ZoneType::Checkout => "checkout",
            ZoneType::WaitingArea => "waiting_area",
            ZoneType::Aisle(_) => "aisle",
            ZoneType::Kitchen => "kitchen",
            ZoneType::LoadingDock => "loading_dock",
            ZoneType::MeetingRoom => "meeting_room",
            ZoneType::Counter => "counter",
            ZoneType::Wall => "wall",
            ZoneType::Custom(s) => s.as_str(),
        }
    }
}

/// A physical spatial region or operational zone where displays are deployed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    /// Unique identifier for the zone (e.g., "entrance", "checkout", "aisle_3").
    pub id: String,
    /// Human-readable label for the zone (e.g., "Main Entrance", "Checkout Counter").
    pub name: String,
    /// Zone category type classification.
    pub kind: ZoneType,
    /// Optional description of the zone's operational role or physical area.
    pub description: Option<String>,
}

impl Zone {
    /// Creates a new custom zone.
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: ZoneType) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            description: None,
        }
    }

    /// Sets the optional description for this zone.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Standard Entrance zone factory.
    #[must_use]
    pub fn entrance() -> Self {
        Self::new("entrance", "Entrance", ZoneType::Entrance)
            .with_description("Main entrance / foyer display")
    }

    /// Standard Checkout zone factory.
    #[must_use]
    pub fn checkout() -> Self {
        Self::new("checkout", "Checkout Counter", ZoneType::Checkout)
            .with_description("Point of sale / checkout screen")
    }

    /// Standard Waiting Area zone factory.
    #[must_use]
    pub fn waiting_area() -> Self {
        Self::new("waiting_area", "Waiting Area", ZoneType::WaitingArea)
            .with_description("Lobby / waiting area screen")
    }

    /// Standard Aisle zone factory (e.g., `Zone::aisle("3")` produces `id: "aisle_3"`).
    #[must_use]
    pub fn aisle(label: impl AsRef<str>) -> Self {
        let label_str = label.as_ref();
        let id = format!("aisle_{label_str}");
        let name = format!("Aisle {label_str}");
        Self::new(id, name, ZoneType::Aisle(label_str.to_string()))
            .with_description(format!("Aisle {label_str} display"))
    }

    /// Standard Kitchen zone factory.
    #[must_use]
    pub fn kitchen() -> Self {
        Self::new("kitchen", "Kitchen", ZoneType::Kitchen)
            .with_description("Kitchen order display / prep area screen")
    }

    /// Standard Loading Dock zone factory.
    #[must_use]
    pub fn loading_dock() -> Self {
        Self::new("loading_dock", "Loading Dock", ZoneType::LoadingDock)
            .with_description("Warehouse loading dock / shipping screen")
    }

    /// Standard Meeting Room zone factory.
    #[must_use]
    pub fn meeting_room() -> Self {
        Self::new("meeting_room", "Meeting Room", ZoneType::MeetingRoom)
            .with_description("Conference / meeting room screen")
    }

    /// Standard Counter zone factory.
    #[must_use]
    pub fn counter() -> Self {
        Self::new("counter", "Service Counter", ZoneType::Counter)
            .with_description("Service counter / pickup display")
    }

    /// Standard Wall zone factory.
    #[must_use]
    pub fn wall() -> Self {
        Self::new("wall", "Feature Wall", ZoneType::Wall)
            .with_description("Feature wall / display array screen")
    }
}

/// Optional spatial grid coordinates for wall or multi-display layout mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GridPosition {
    /// Row index in grid layout (0-indexed).
    pub row: u32,
    /// Column index in grid layout (0-indexed).
    pub col: u32,
    /// X coordinate in layout space or relative offset.
    pub x: i32,
    /// Y coordinate in layout space or relative offset.
    pub y: i32,
    /// Display orientation angle in degrees (0, 90, 180, 270).
    pub rotation_degrees: u32,
}

/// A display mapped within a physical display topology, binding hardware properties to a zone and role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappedDisplay {
    /// Unique identifier for this mapping node.
    pub id: String,
    /// Target zone assignment.
    pub zone: Zone,
    /// Human-centric semantic role or location context (e.g. "the entrance display in the restaurant").
    pub role: String,
    /// Network IP address, if applicable (e.g., "192.168.1.45").
    pub ip_address: Option<String>,
    /// System display connector name, if attached locally (e.g., "HDMI-1", "\\\\.\\DISPLAY1").
    pub system_name: Option<String>,
    /// EDID serial number or product hardware ID for hardware identity matching.
    pub edid_serial: Option<String>,
    /// Probed hardware facts, if available from local probing.
    pub probe_info: Option<ProbeInfo>,
    /// Optional spatial grid position for layout rendering.
    pub position: Option<GridPosition>,
    /// Arbitrary tags for filtering or scenario grouping (e.g., ["menu_board", "4k"]).
    pub tags: Vec<String>,
}

impl MappedDisplay {
    /// Creates a new mapped display binding an ID, zone, and semantic role.
    pub fn new(id: impl Into<String>, zone: Zone, role: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            zone,
            role: role.into(),
            ip_address: None,
            system_name: None,
            edid_serial: None,
            probe_info: None,
            position: None,
            tags: Vec::new(),
        }
    }

    /// Builder method to associate an IP address.
    #[must_use]
    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }

    /// Builder method to associate a system connector name.
    #[must_use]
    pub fn with_system_name(mut self, name: impl Into<String>) -> Self {
        self.system_name = Some(name.into());
        self
    }

    /// Builder method to associate an EDID serial number.
    #[must_use]
    pub fn with_edid_serial(mut self, serial: impl Into<String>) -> Self {
        self.edid_serial = Some(serial.into());
        self
    }

    /// Builder method to attach probed hardware info.
    #[must_use]
    pub fn with_probe_info(mut self, info: ProbeInfo) -> Self {
        if self.system_name.is_none() && info.system_name.is_some() {
            self.system_name = info.system_name.clone();
        }
        if self.edid_serial.is_none() && info.serial.is_some() {
            self.edid_serial = info.serial.clone();
        }
        self.probe_info = Some(info);
        self
    }

    /// Builder method to set grid position coordinates.
    #[must_use]
    pub fn with_position(mut self, pos: GridPosition) -> Self {
        self.position = Some(pos);
        self
    }

    /// Builder method to add a tag.
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Formats a complete human-centric description of the display and its location.
    /// E.g. `"the entrance display in the restaurant (192.168.1.45 / HDMI-1)"` or `"the entrance display in the restaurant"`.
    #[must_use]
    pub fn semantic_label(&self) -> String {
        let mut hardware_parts = Vec::new();
        if let Some(ref ip) = self.ip_address {
            hardware_parts.push(ip.as_str());
        }
        if let Some(ref sys) = self.system_name {
            hardware_parts.push(sys.as_str());
        }
        if hardware_parts.is_empty() {
            self.role.clone()
        } else {
            format!("{} ({})", self.role, hardware_parts.join(" / "))
        }
    }
}

/// Topology manager tracking physical zones and mapped displays across local systems and network endpoints.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DisplayTopology {
    /// Registered zones indexed by zone ID.
    pub zones: HashMap<String, Zone>,
    /// Mapped displays.
    pub displays: Vec<MappedDisplay>,
}

impl DisplayTopology {
    /// Creates an empty display topology.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a zone in the topology.
    pub fn register_zone(&mut self, zone: Zone) {
        self.zones.insert(zone.id.clone(), zone);
    }

    /// Registers standard venue zones (entrance, checkout, waiting_area, kitchen, loading_dock, meeting_room, counter, wall).
    pub fn register_standard_venue_zones(&mut self) {
        self.register_zone(Zone::entrance());
        self.register_zone(Zone::checkout());
        self.register_zone(Zone::waiting_area());
        self.register_zone(Zone::kitchen());
        self.register_zone(Zone::loading_dock());
        self.register_zone(Zone::meeting_room());
        self.register_zone(Zone::counter());
        self.register_zone(Zone::wall());
    }

    /// Maps a screen/display into the topology.
    pub fn map_display(&mut self, display: MappedDisplay) {
        // Also ensure the display's zone is registered in the topology
        self.zones
            .entry(display.zone.id.clone())
            .or_insert_with(|| display.zone.clone());

        if let Some(idx) = self.displays.iter().position(|d| d.id == display.id) {
            self.displays[idx] = display;
        } else {
            self.displays.push(display);
        }
    }

    /// Unmaps a display by ID.
    pub fn unmap_display(&mut self, id: &str) -> Option<MappedDisplay> {
        if let Some(idx) = self.displays.iter().position(|d| d.id == id) {
            Some(self.displays.remove(idx))
        } else {
            None
        }
    }

    /// Retrieves a mapped display by ID.
    #[must_use]
    pub fn get_display(&self, id: &str) -> Option<&MappedDisplay> {
        self.displays.iter().find(|d| d.id == id)
    }

    /// Finds displays mapped to a specific zone ID.
    #[must_use]
    pub fn find_by_zone(&self, zone_id: &str) -> Vec<&MappedDisplay> {
        self.displays
            .iter()
            .filter(|d| d.zone.id == zone_id)
            .collect()
    }

    /// Finds a mapped display by IP address.
    #[must_use]
    pub fn find_by_ip(&self, ip: &str) -> Option<&MappedDisplay> {
        self.displays
            .iter()
            .find(|d| d.ip_address.as_deref() == Some(ip))
    }

    /// Finds a mapped display by system connector name (e.g., "HDMI-1").
    #[must_use]
    pub fn find_by_system_name(&self, system_name: &str) -> Option<&MappedDisplay> {
        self.displays.iter().find(|d| {
            d.system_name
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(system_name))
        })
    }

    /// Finds a mapped display by EDID serial number.
    #[must_use]
    pub fn find_by_serial(&self, serial: &str) -> Option<&MappedDisplay> {
        self.displays
            .iter()
            .find(|d| d.edid_serial.as_deref() == Some(serial))
    }

    /// Automatically maps attached local monitors from `ProbeInfo` slices.
    /// Existing mappings matching `system_name` or `serial` are updated with the latest `ProbeInfo`.
    /// Unmatched monitors are assigned a default zone based on heuristics or general default.
    pub fn auto_map_probes(&mut self, probes: &[ProbeInfo]) {
        for (i, probe) in probes.iter().enumerate() {
            let matched_idx = self.displays.iter().position(|d| {
                (d.system_name.is_some() && d.system_name == probe.system_name)
                    || (d.edid_serial.is_some()
                        && probe.serial.is_some()
                        && d.edid_serial == probe.serial)
            });

            if let Some(idx) = matched_idx {
                self.displays[idx].probe_info = Some(probe.clone());
            } else {
                let sys_name = probe.system_name.as_deref().unwrap_or("local_display");
                let display_id = format!("auto_screen_{}", i + 1);
                let default_zone = Zone::new(
                    "general",
                    "General Area",
                    ZoneType::Custom("general".to_string()),
                );
                let role = format!("local display {} ({sys_name})", i + 1);

                let mapped = MappedDisplay::new(display_id, default_zone, role)
                    .with_probe_info(probe.clone());
                self.map_display(mapped);
            }
        }
    }

    /// Returns a list of semantic strings describing every mapped display in the topology.
    #[must_use]
    pub fn semantic_descriptions(&self) -> Vec<String> {
        self.displays.iter().map(|d| d.semantic_label()).collect()
    }

    /// Serializes the topology map to a JSON string.
    ///
    /// # Errors
    /// Returns a `serde_json::Error` if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes a topology map from a JSON string.
    ///
    /// # Errors
    /// Returns a `serde_json::Error` if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Attachment;

    #[test]
    fn zone_factories_and_kinds() {
        let entrance = Zone::entrance();
        assert_eq!(entrance.id, "entrance");
        assert_eq!(entrance.kind, ZoneType::Entrance);
        assert_eq!(entrance.kind.as_str(), "entrance");

        let checkout = Zone::checkout();
        assert_eq!(checkout.id, "checkout");
        assert_eq!(checkout.kind, ZoneType::Checkout);

        let aisle3 = Zone::aisle("3");
        assert_eq!(aisle3.id, "aisle_3");
        assert_eq!(aisle3.kind, ZoneType::Aisle("3".to_string()));

        let kitchen = Zone::kitchen();
        assert_eq!(kitchen.id, "kitchen");
        assert_eq!(kitchen.kind, ZoneType::Kitchen);

        let dock = Zone::loading_dock();
        assert_eq!(dock.id, "loading_dock");
        assert_eq!(dock.kind, ZoneType::LoadingDock);

        let meeting = Zone::meeting_room();
        assert_eq!(meeting.id, "meeting_room");
        assert_eq!(meeting.kind, ZoneType::MeetingRoom);

        let waiting = Zone::waiting_area();
        assert_eq!(waiting.id, "waiting_area");
        assert_eq!(waiting.kind, ZoneType::WaitingArea);

        let counter = Zone::counter();
        assert_eq!(counter.id, "counter");
        assert_eq!(counter.kind, ZoneType::Counter);

        let wall = Zone::wall();
        assert_eq!(wall.id, "wall");
        assert_eq!(wall.kind, ZoneType::Wall);
    }

    #[test]
    fn mapped_display_semantic_label() {
        let display = MappedDisplay::new(
            "screen_01",
            Zone::entrance(),
            "the entrance display in the restaurant",
        )
        .with_ip("192.168.1.45")
        .with_system_name("HDMI-1")
        .with_edid_serial("SAM123456");

        assert_eq!(
            display.semantic_label(),
            "the entrance display in the restaurant (192.168.1.45 / HDMI-1)"
        );

        let no_hw = MappedDisplay::new("screen_02", Zone::kitchen(), "kitchen order display");
        assert_eq!(no_hw.semantic_label(), "kitchen order display");
    }

    #[test]
    fn topology_mapping_and_lookups() {
        let mut topo = DisplayTopology::new();
        topo.register_standard_venue_zones();

        let entrance_tv = MappedDisplay::new(
            "tv_entrance",
            Zone::entrance(),
            "the entrance display in the restaurant",
        )
        .with_ip("192.168.1.45")
        .with_system_name("HDMI-1")
        .with_edid_serial("SAM_ENTRANCE_001")
        .with_tag("menu_board");

        let kitchen_tv = MappedDisplay::new("tv_kitchen", Zone::kitchen(), "kitchen order display")
            .with_ip("192.168.1.46")
            .with_system_name("HDMI-2")
            .with_edid_serial("LGE_KITCHEN_002");

        let aisle3_tv = MappedDisplay::new("tv_aisle3", Zone::aisle("3"), "aisle 3 promo screen")
            .with_ip("192.168.1.47");

        topo.map_display(entrance_tv);
        topo.map_display(kitchen_tv);
        topo.map_display(aisle3_tv);

        assert_eq!(topo.displays.len(), 3);

        // Lookups
        assert_eq!(
            topo.find_by_ip("192.168.1.45").unwrap().role,
            "the entrance display in the restaurant"
        );
        assert_eq!(
            topo.find_by_system_name("hdmi-1").unwrap().id,
            "tv_entrance"
        );
        assert_eq!(
            topo.find_by_serial("LGE_KITCHEN_002").unwrap().id,
            "tv_kitchen"
        );

        let kitchen_displays = topo.find_by_zone("kitchen");
        assert_eq!(kitchen_displays.len(), 1);
        assert_eq!(kitchen_displays[0].id, "tv_kitchen");

        let descriptions = topo.semantic_descriptions();
        assert_eq!(descriptions.len(), 3);
        assert!(descriptions.contains(
            &"the entrance display in the restaurant (192.168.1.45 / HDMI-1)".to_string()
        ));
    }

    #[test]
    fn auto_map_probes_integrates_hardware_info() {
        let mut topo = DisplayTopology::new();
        let probe1 = ProbeInfo {
            system_name: Some("HDMI-1".to_string()),
            width: 3840,
            height: 2160,
            vendor: Some("SAM".to_string()),
            serial: Some("SN_SAM_123".to_string()),
            attachment: Attachment::Hdmi,
            is_tv: true,
            tv_confidence: 0.9,
            ..ProbeInfo::default()
        };

        topo.auto_map_probes(std::slice::from_ref(&probe1));
        assert_eq!(topo.displays.len(), 1);
        let auto_display = &topo.displays[0];
        assert_eq!(auto_display.system_name.as_deref(), Some("HDMI-1"));
        assert_eq!(auto_display.edid_serial.as_deref(), Some("SN_SAM_123"));
        assert!(auto_display.probe_info.is_some());

        // ProbeInfo helper into_mapped_display
        let mapped = probe1.into_mapped_display(
            "custom_entrance",
            Zone::entrance(),
            "the entrance display in the restaurant",
        );
        assert_eq!(mapped.zone.id, "entrance");
        assert_eq!(mapped.system_name.as_deref(), Some("HDMI-1"));
    }

    #[test]
    fn json_serialization_roundtrip() {
        let mut topo = DisplayTopology::new();
        topo.map_display(
            MappedDisplay::new("d1", Zone::checkout(), "checkout screen 1")
                .with_ip("192.168.1.100")
                .with_position(GridPosition {
                    row: 0,
                    col: 1,
                    x: 100,
                    y: 200,
                    rotation_degrees: 0,
                }),
        );

        let json = topo.to_json().expect("serialize topology");
        assert!(json.contains("checkout"));
        assert!(json.contains("192.168.1.100"));

        let restored = DisplayTopology::from_json(&json).expect("deserialize topology");
        assert_eq!(topo, restored);
    }
}
