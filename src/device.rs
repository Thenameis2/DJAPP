use std::{error::Error, str::FromStr};

use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device, DeviceId, InterfaceType, SampleFormat, SupportedStreamConfig,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub interface: String,
    pub channels: u16,
    pub max_channels: u16,
    pub sample_rate: u32,
    pub stereo_master_supported: bool,
    pub stereo_cue_supported: bool,
    pub routing_mode: String,
    pub limitation: Option<String>,
}

pub fn output_devices() -> Result<Vec<OutputDeviceInfo>, Box<dyn Error>> {
    let host = cpal::default_host();
    let default_id = host
        .default_output_device()
        .and_then(|device| device.id().ok());
    let mut devices = Vec::new();

    for device in host.output_devices()? {
        let id = device.id()?;
        let description = device.description()?;
        let config = device.default_output_config()?;
        let max_channels = device
            .supported_output_configs()?
            .map(|config| config.channels())
            .max()
            .unwrap_or(config.channels());
        let stereo_master_supported = max_channels >= 2;
        let stereo_cue_supported = max_channels >= 4;
        devices.push(OutputDeviceInfo {
            id: id.to_string(),
            name: description.name().to_string(),
            is_default: default_id.as_ref() == Some(&id),
            interface: interface_name(description.interface_type()).to_string(),
            channels: config.channels(),
            max_channels,
            sample_rate: config.sample_rate(),
            stereo_master_supported,
            stereo_cue_supported,
            routing_mode: if stereo_cue_supported {
                "master-and-cue"
            } else {
                "master-only"
            }
            .to_string(),
            limitation: (!stereo_cue_supported).then(|| {
                "Stereo headphone cue requires one output device with at least four channels."
                    .to_string()
            }),
        });
    }

    devices.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(devices)
}

pub(crate) fn preferred_output_config(
    device: &Device,
) -> Result<SupportedStreamConfig, Box<dyn Error>> {
    let default = device.default_output_config()?;
    let default_rate = default.sample_rate();
    let mut candidates = device
        .supported_output_configs()?
        .filter(|config| config.channels() >= 4)
        .collect::<Vec<_>>();
    candidates.sort_by_key(|config| {
        (
            config.channels(),
            config.sample_format() == SampleFormat::F32,
        )
    });
    let Some(candidate) = candidates.pop() else {
        return Ok(default);
    };
    let rate = if candidate.min_sample_rate() <= default_rate
        && default_rate <= candidate.max_sample_rate()
    {
        default_rate
    } else {
        candidate.max_sample_rate().min(48_000)
    };
    Ok(candidate.with_sample_rate(rate))
}

pub(crate) fn resolve_output_device(id: Option<&str>) -> Result<Device, Box<dyn Error>> {
    let host = cpal::default_host();
    match id {
        Some(id) => {
            let id = DeviceId::from_str(id)?;
            host.device_by_id(&id)
                .ok_or_else(|| format!("selected output device is unavailable: {id}").into())
        }
        None => host
            .default_output_device()
            .ok_or_else(|| "no default output device".into()),
    }
}

fn interface_name(interface: InterfaceType) -> &'static str {
    match interface {
        InterfaceType::BuiltIn => "built-in",
        InterfaceType::Usb => "usb",
        InterfaceType::Bluetooth => "bluetooth",
        InterfaceType::Pci => "pci",
        InterfaceType::FireWire => "firewire",
        InterfaceType::Thunderbolt => "thunderbolt",
        InterfaceType::Hdmi => "hdmi",
        InterfaceType::Line => "line",
        InterfaceType::Spdif => "spdif",
        InterfaceType::Network => "network",
        InterfaceType::Virtual => "virtual",
        InterfaceType::DisplayPort => "display-port",
        InterfaceType::Aggregate => "aggregate",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn routing_capability_thresholds_are_explicit() {
        for (channels, master, cue) in [
            (1, false, false),
            (2, true, false),
            (4, true, true),
            (8, true, true),
        ] {
            assert_eq!(channels >= 2, master);
            assert_eq!(channels >= 4, cue);
        }
    }
}
