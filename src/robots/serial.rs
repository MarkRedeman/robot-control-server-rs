use anyhow::{Context, Result, bail};
use serialport::SerialPortType;

/// Find a serial port by its USB device serial number.
///
/// Enumerates all available serial ports and returns the device path
/// of the one matching the given serial number.
pub fn find_port_by_serial_number(serial_number: &str) -> Result<String> {
    let ports = serialport::available_ports().context("Failed to enumerate serial ports")?;

    let mut usb_ports = Vec::new();

    for port in &ports {
        if let SerialPortType::UsbPort(usb_info) = &port.port_type {
            usb_ports.push((&port.port_name, usb_info));
            if let Some(ref sn) = usb_info.serial_number
                && sn == serial_number
            {
                return Ok(port.port_name.clone());
            }
        }
    }

    if usb_ports.is_empty() {
        bail!("No USB serial ports found on this system");
    }

    let mut msg = format!(
        "No USB serial port found with serial number '{serial_number}'.\n\nAvailable USB serial ports:\n"
    );
    for (name, info) in &usb_ports {
        let sn = info
            .serial_number
            .as_deref()
            .unwrap_or("<no serial number>");
        let product = info.product.as_deref().unwrap_or("<unknown>");
        let manufacturer = info.manufacturer.as_deref().unwrap_or("<unknown>");
        msg.push_str(&format!(
            "  {name}  serial={sn}  product={product}  manufacturer={manufacturer}\n"
        ));
    }

    bail!("{msg}")
}

/// List all available serial ports for diagnostics.
pub fn list_available_ports() -> Result<Vec<String>> {
    let ports = serialport::available_ports().context("Failed to enumerate serial ports")?;
    Ok(ports.into_iter().map(|p| p.port_name).collect())
}
