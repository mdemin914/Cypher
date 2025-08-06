use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host, HostId};


fn get_host_from_id(host_id: HostId) -> Result<Host> {
    cpal::host_from_id(host_id).map_err(|e| anyhow::anyhow!("Failed to get audio host: {}", e))
}

pub fn get_input_devices(host_id: HostId) -> Result<Vec<(String, Device)>> {
    let host = get_host_from_id(host_id)?;
    let devices = host.input_devices()?;
    let mut result = Vec::new();
    for device in devices {
        if let Ok(name) = device.name() {
            result.push((name, device));
        }
    }
    Ok(result)
}

pub fn get_output_devices(host_id: HostId) -> Result<Vec<(String, Device)>> {
    let host = get_host_from_id(host_id)?;
    let devices = host.output_devices()?;
    let mut result = Vec::new();
    for device in devices {
        if let Ok(name) = device.name() {
            result.push((name, device));
        }
    }
    Ok(result)
}