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
        Cell::new("Raw Pos"),
        Cell::new("Pos (rad)"),
        Cell::new("Pos (deg)"),
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
        let position_rad = (2.0 * std::f64::consts::PI * (f64::from(js.raw_position)) / 4096.0)
            - std::f64::consts::PI;
        let position_deg = position_rad.to_degrees();

        let mut row = vec![
            Cell::new(js.joint.to_string()),
            Cell::new(js.motor_id),
            Cell::new(js.raw_position),
            Cell::new(format!("{:>8.3}", position_rad)),
            Cell::new(format!("{:>8.1}", position_deg)),
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
