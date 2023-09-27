use std::clone::Clone;

use libpulse_binding::{context::introspect, proplist::Proplist};

#[derive(Debug)]
pub struct ServerInfo {
    pub user_name: Option<String>,
    pub host_name: Option<String>,
    pub server_version: Option<String>,
    pub server_name: Option<String>,
    // pub sample_spec: sample::Spec,
    pub default_sink_name: Option<String>,
    pub default_source_name: Option<String>,
    pub cookie: u32,
    // pub channel_map: channelmap::Map,
}

impl<'a> From<&'a introspect::ServerInfo<'a>> for ServerInfo {
    fn from(info: &'a introspect::ServerInfo<'a>) -> Self {
        ServerInfo {
            user_name: info.user_name.as_ref().map(|cow| cow.to_string()),
            host_name: info.host_name.as_ref().map(|cow| cow.to_string()),
            server_version: info.server_version.as_ref().map(|cow| cow.to_string()),
            server_name: info.server_name.as_ref().map(|cow| cow.to_string()),
            // sample_spec: info.sample_spec,
            default_sink_name: info.default_sink_name.as_ref().map(|cow| cow.to_string()),
            default_source_name: info.default_source_name.as_ref().map(|cow| cow.to_string()),
            cookie: info.cookie,
            // channel_map: info.channel_map,
        }
    }
}

// See https://github.com/halli2/pulsectl-rs/blob/main/src/controllers/types.rs for reference.
#[derive(Debug)]
pub struct DeviceInfo {
    pub index: u32,
    pub name: Option<String>,
    pub description: Option<String>,
    // pub sample_spec: sample::Spec,
    // pub channel_map: channelmap::Map,
    // pub owner_module: Option<u32>,
    // pub volume: ChannelVolumes,
    // pub mute: bool,
    // pub monitor: Option<u32>,
    // pub monitor_name: Option<String>,
    // pub latency: MicroSeconds,
    // pub driver: Option<String>,
    // pub flags: Flags,
    // pub proplist: Proplist,
    // pub configured_latency: MicroSeconds,
    // pub base_volume: Volume,
    // pub state: DevState,
    // pub n_volume_steps: u32,
    // pub card: Option<u32>,
    // pub ports: Vec<DevicePortInfo>,
    // pub active_port: Option<DevicePortInfo>,
    // pub formats: Vec<format::Info>,
}

impl<'a> From<&'a introspect::SinkInfo<'a>> for DeviceInfo {
    fn from(item: &'a introspect::SinkInfo<'a>) -> Self {
        DeviceInfo {
            name: item.name.as_ref().map(|cow| cow.to_string()),
            index: item.index,
            description: item.description.as_ref().map(|cow| cow.to_string()),
            // sample_spec: item.sample_spec,
            // channel_map: item.channel_map,
            // owner_module: item.owner_module,
            // volume: item.volume,
            // mute: item.mute,
            // monitor: Some(item.monitor_source),
            // monitor_name: item.monitor_source_name.as_ref().map(|cow| cow.to_string()),
            // latency: item.latency,
            // driver: item.driver.as_ref().map(|cow| cow.to_string()),
            // flags: Flags::SinkFlags(item.flags),
            // proplist: item.proplist.clone(),
            // configured_latency: item.configured_latency,
            // base_volume: item.base_volume,
            // state: DevState::from(item.state),
            // n_volume_steps: item.n_volume_steps,
            // card: item.card,
            // ports: item.ports.iter().map(From::from).collect(),
            // active_port: item.active_port.as_ref().map(From::from),
            // formats: item.formats.clone(),
        }
    }
}

impl<'a> From<&'a introspect::SourceInfo<'a>> for DeviceInfo {
    fn from(item: &'a introspect::SourceInfo<'a>) -> Self {
        DeviceInfo {
            name: item.name.as_ref().map(|cow| cow.to_string()),
            index: item.index,
            description: item.description.as_ref().map(|cow| cow.to_string()),
            // sample_spec: item.sample_spec,
            // channel_map: item.channel_map,
            // owner_module: item.owner_module,
            // volume: item.volume,
            // mute: item.mute,
            // monitor: item.monitor_of_sink,
            // monitor_name: item
            //     .monitor_of_sink_name
            //     .as_ref()
            //     .map(|cow| cow.to_string()),
            // latency: item.latency,
            // driver: item.driver.as_ref().map(|cow| cow.to_string()),
            // flags: Flags::SourceFLags(item.flags),
            // proplist: item.proplist.clone(),
            // configured_latency: item.configured_latency,
            // base_volume: item.base_volume,
            // state: DevState::from(item.state),
            // n_volume_steps: item.n_volume_steps,
            // card: item.card,
            // ports: item.ports.iter().map(From::from).collect(),
            // active_port: item.active_port.as_ref().map(From::from),
            // formats: item.formats.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApplicationInfo {
    pub index: u32,
    pub name: Option<String>,
    pub owner_module: Option<u32>,
    pub client: Option<u32>,
    pub connection_id: u32,
    // pub sample_spec: sample::Spec,
    // pub channel_map: channelmap::Map,
    // pub volume: ChannelVolumes,
    // pub buffer_usec: MicroSeconds,
    // pub connection_usec: MicroSeconds,
    pub resample_method: Option<String>,
    pub driver: Option<String>,
    pub mute: bool,
    pub proplist: Proplist,
    pub corked: bool,
    pub has_volume: bool,
    pub volume_writable: bool,
    // pub format: format::Info,
}

impl<'a> From<&'a introspect::SinkInputInfo<'a>> for ApplicationInfo {
    fn from(item: &'a introspect::SinkInputInfo<'a>) -> Self {
        ApplicationInfo {
            index: item.index,
            name: item.name.as_ref().map(|cow| cow.to_string()),
            owner_module: item.owner_module,
            client: item.client,
            connection_id: item.sink,
            // sample_spec: item.sample_spec,
            // channel_map: item.channel_map,
            // volume: item.volume,
            // buffer_usec: item.buffer_usec,
            // connection_usec: item.sink_usec,
            resample_method: item.resample_method.as_ref().map(|cow| cow.to_string()),
            driver: item.driver.as_ref().map(|cow| cow.to_string()),
            mute: item.mute,
            proplist: item.proplist.clone(),
            corked: item.corked,
            has_volume: item.has_volume,
            volume_writable: item.volume_writable,
            // format: item.format.clone(),
        }
    }
}

impl<'a> From<&'a introspect::SourceOutputInfo<'a>> for ApplicationInfo {
    fn from(item: &'a introspect::SourceOutputInfo<'a>) -> Self {
        ApplicationInfo {
            index: item.index,
            name: item.name.as_ref().map(|cow| cow.to_string()),
            owner_module: item.owner_module,
            client: item.client,
            connection_id: item.source,
            // sample_spec: item.sample_spec,
            // channel_map: item.channel_map,
            // volume: item.volume,
            // buffer_usec: item.buffer_usec,
            // connection_usec: item.source_usec,
            resample_method: item.resample_method.as_ref().map(|cow| cow.to_string()),
            driver: item.driver.as_ref().map(|cow| cow.to_string()),
            mute: item.mute,
            proplist: item.proplist.clone(),
            corked: item.corked,
            has_volume: item.has_volume,
            volume_writable: item.volume_writable,
            // format: item.format.clone(),
        }
    }
}
