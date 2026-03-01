use crate::robots::client::Joint;
use crate::robots::ArmState;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, ContentArrangement, Table};

/// Render an ArmState as a pretty table string, labelled with the given identifier.
///
/// When any joint has calibration data, an extra "Joint Angle" column is appended
/// (showing degrees for body joints, percentage for the gripper).
pub fn format_arm_state(label: &str, state: &ArmState) -> String {
    let has_calibration = state.joints.iter().any(|j| j.calibrated_angle.is_some());

    let mut header = vec![
        Cell::new("Joint"),
        Cell::new("ID"),
        Cell::new("Pos (rad)"),
        Cell::new("Pos (deg)"),
        Cell::new("Speed (rad/s)"),
        Cell::new("Load"),
        Cell::new("Voltage"),
        Cell::new("Temp (C)"),
        Cell::new("Moving"),
    ];
    if has_calibration {
        header.push(Cell::new("Joint Angle"));
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(header);

    for js in &state.joints {
        let mut row = vec![
            Cell::new(js.joint.to_string()),
            Cell::new(js.motor_id),
            Cell::new(format!("{:>8.3}", js.position_rad)),
            Cell::new(format!("{:>8.1}", js.position_deg)),
            Cell::new(format!("{:>8.3}", js.speed_rad_s)),
            Cell::new(js.load),
            Cell::new(js.voltage),
            Cell::new(js.temperature),
            Cell::new(if js.moving { "Yes" } else { "No" }),
        ];
        if has_calibration {
            let angle_str = match js.calibrated_angle {
                Some(v) => {
                    if js.joint == Joint::Gripper.name() {
                        format!("{v:>6.1}%")
                    } else {
                        format!("{v:>8.2}°")
                    }
                }
                None => "-".to_string(),
            };
            row.push(Cell::new(angle_str));
        }
        table.add_row(row);
    }

    format!("SO101 Arm State [{label}] ({})\n{table}", state.timestamp)
}
