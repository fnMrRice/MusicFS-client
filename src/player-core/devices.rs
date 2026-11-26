use crate::processing::MAX_CHANNELS;
use rodio::cpal::{
    self,
    traits::{DeviceTrait, HostTrait},
    Device, SupportedStreamConfig,
};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct OutputDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub max_channels: Option<u16>,
}

struct Endpoint {
    id: String,
    name: String,
    device: Device,
    is_default: bool,
}

fn endpoints() -> Result<Vec<Endpoint>, String> {
    let host = cpal::default_host();
    let backend = host.id().name();
    let default = host.default_output_device();
    let devices: Vec<_> = host
        .output_devices()
        .map_err(|e| format!("Cannot list audio output devices: {e}"))?
        .filter_map(|device| Some((device.name().ok()?, device)))
        .collect();
    // CPAL 0.16 只公开设备名称。同名设备单独编号，数量变化时让旧选择失效。
    let mut counts = HashMap::new();
    for (name, _) in &devices {
        *counts.entry(name.clone()).or_insert(0usize) += 1;
    }
    let mut seen = HashMap::new();
    let mut result = Vec::with_capacity(devices.len());
    for (name, device) in devices {
        let count = counts[&name];
        let ordinal = seen.entry(name.clone()).or_insert(0usize);
        *ordinal += 1;
        let id = format!("{backend}:{count}:{ordinal}:{name}");
        #[cfg(windows)]
        let is_default = default
            .as_ref()
            .is_some_and(|d| same_stream_device(d, &device));
        #[cfg(not(windows))]
        let is_default = count == 1
            && default
                .as_ref()
                .is_some_and(|d| d.name().is_ok_and(|n| n == name));
        let name = if count > 1 {
            format!("{name} ({ordinal})")
        } else {
            name
        };
        result.push(Endpoint {
            id,
            name,
            device,
            is_default,
        });
    }
    Ok(result)
}

#[cfg(windows)]
pub(crate) fn same_stream_device(a: &Device, b: &Device) -> bool {
    use cpal::platform::DeviceInner::Wasapi;
    // Windows 后端直接比较 WASAPI 端点 ID，同名设备也能识别系统默认项。
    #[allow(unreachable_patterns)]
    match (a.as_inner(), b.as_inner()) {
        (Wasapi(a), Wasapi(b)) => a == b,
        _ => false,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn same_stream_device(a: &Device, b: &Device) -> bool {
    use cpal::platform::DeviceInner::CoreAudio;
    let (CoreAudio(a), CoreAudio(b)) = (a.as_inner(), b.as_inner());
    // CoreAudio 的默认输出与固定输出使用不同 AudioUnit，比较中包含这种区别。
    a == b
}

pub fn output_devices() -> Result<Vec<OutputDeviceInfo>, String> {
    Ok(endpoints()?
        .into_iter()
        .map(|e| OutputDeviceInfo {
            max_channels: stream_config(&e.device).ok().map(|c| c.channels()),
            id: e.id,
            name: e.name,
            is_default: e.is_default,
        })
        .collect())
}

pub(crate) fn resolve(id: Option<&str>) -> Result<Device, String> {
    match id {
        Some(id) => endpoints()?
            .into_iter()
            .find(|d| d.id == id)
            .map(|d| d.device)
            .ok_or_else(|| "Selected audio output device is unavailable.".into()),
        None => cpal::default_host()
            .default_output_device()
            .ok_or_else(|| "System default audio output device is unavailable.".into()),
    }
}

pub(crate) fn stream_config(device: &Device) -> Result<SupportedStreamConfig, String> {
    let default = device.default_output_config().map_err(|e| e.to_string())?;
    let mut best = (usize::from(default.channels()) <= MAX_CHANNELS).then(|| default.clone());
    if let Ok(configs) = device.supported_output_configs() {
        for config in configs {
            let channels = config.channels();
            if usize::from(channels) <= MAX_CHANNELS
                && best.as_ref().is_none_or(|b| channels > b.channels())
            {
                let rate = default
                    .sample_rate()
                    .clamp(config.min_sample_rate(), config.max_sample_rate());
                best = Some(config.with_sample_rate(rate));
            }
        }
    }
    best.ok_or_else(|| "Audio output supports up to 32 channels.".into())
}

pub fn device_channels() -> Result<u16, String> {
    stream_config(&resolve(None)?).map(|c| c.channels())
}
