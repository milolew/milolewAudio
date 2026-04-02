//! Device Rack View — horizontal chain of effect devices with parameter knobs.
//!
//! Layout: left-to-right chain of `DeviceSlot` widgets, each showing
//! an effect name, bypass indicator, and rotary knobs for parameters.
//!
//! ```text
//! +----------+   +----------+   +----------+
//! | EQ       |   | Comp     |   | Reverb   |
//! | [K] [K]  |-->| [K] [K]  |-->| [K] [K]  |
//! | [K]      |   | [K]      |   | [K] [K]  |
//! +----------+   +----------+   +----------+
//! ```
//!
//! Currently uses demo data. When `DeviceRackState` is added to `AppData`
//! (integration worktree), this view will read from lenses instead.

pub mod device_slot;

use vizia::prelude::*;

use crate::app_data::{AppData, AppEvent};

use self::device_slot::{DeviceSlot, DeviceSlotData, DeviceSlotEvent, ParameterData};

/// Device rack view — renders an L→R effect chain for the selected track.
pub struct DeviceRackView;

impl DeviceRackView {
    pub fn new(cx: &mut Context) -> Handle<'_, Self> {
        Self.build(cx, |cx| {
            // Header
            Label::new(cx, "Device Rack").class("rack-title");

            // L→R device chain (demo data — will be replaced by AppData lens)
            HStack::new(cx, |cx| {
                let demo_devices = demo_device_chain();
                let device_count = demo_devices.len();
                for (idx, device) in demo_devices.into_iter().enumerate() {
                    DeviceSlot::new(cx, idx, device)
                        .width(Pixels(140.0))
                        .height(Pixels(120.0));

                    // Arrow separator between devices (not after last)
                    if idx + 1 < device_count {
                        Label::new(cx, "\u{2192}").class("chain-arrow");
                    }
                }
            })
            .class("device-chain");
        })
    }
}

impl View for DeviceRackView {
    fn element(&self) -> Option<&'static str> {
        Some("device-rack-view")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|slot_event, _meta| {
            // Get the selected track ID for routing bypass commands
            let track_id = cx
                .data::<AppData>()
                .and_then(|app| app.arrangement.selected_track);

            match slot_event {
                DeviceSlotEvent::ParameterChanged { .. } => {
                    // Parameter changes are handled locally in DeviceSlot for now
                }
                DeviceSlotEvent::ToggleBypass { device_index } => {
                    if let Some(track_id) = track_id {
                        cx.emit(AppEvent::ToggleDeviceBypass {
                            track_id,
                            device_index: *device_index,
                        });
                    }
                }
            }
        });
    }
}

/// Demo device chain for visual development.
/// Will be replaced by AppData lens when DeviceRackState is integrated.
fn demo_device_chain() -> Vec<DeviceSlotData> {
    vec![
        DeviceSlotData {
            name: "EQ".to_string(),
            bypassed: false,
            color: [100, 180, 220],
            parameters: vec![
                ParameterData {
                    name: "Low".to_string(),
                    value: 0.5,
                    default_value: 0.5,
                },
                ParameterData {
                    name: "Mid".to_string(),
                    value: 0.5,
                    default_value: 0.5,
                },
                ParameterData {
                    name: "High".to_string(),
                    value: 0.6,
                    default_value: 0.5,
                },
            ],
        },
        DeviceSlotData {
            name: "Compressor".to_string(),
            bypassed: false,
            color: [220, 160, 80],
            parameters: vec![
                ParameterData {
                    name: "Thresh".to_string(),
                    value: 0.7,
                    default_value: 0.7,
                },
                ParameterData {
                    name: "Ratio".to_string(),
                    value: 0.3,
                    default_value: 0.25,
                },
                ParameterData {
                    name: "Attack".to_string(),
                    value: 0.2,
                    default_value: 0.2,
                },
                ParameterData {
                    name: "Release".to_string(),
                    value: 0.5,
                    default_value: 0.5,
                },
            ],
        },
        DeviceSlotData {
            name: "Reverb".to_string(),
            bypassed: true,
            color: [140, 200, 140],
            parameters: vec![
                ParameterData {
                    name: "Size".to_string(),
                    value: 0.6,
                    default_value: 0.5,
                },
                ParameterData {
                    name: "Decay".to_string(),
                    value: 0.4,
                    default_value: 0.3,
                },
                ParameterData {
                    name: "Mix".to_string(),
                    value: 0.25,
                    default_value: 0.2,
                },
            ],
        },
    ]
}
