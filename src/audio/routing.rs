use pulsectl::controllers::{
    types::{ApplicationInfo, DeviceInfo},
    AppControl, DeviceControl, SinkController, SourceController,
};

use crate::error::Error;

fn find_device_by_name<'a>(
    devices: &'a [DeviceInfo],
    searched_name: &str,
) -> Option<&'a DeviceInfo> {
    devices.iter().find(|device| {
        device
            .name
            .as_ref()
            .is_some_and(|device_name| device_name == searched_name)
    })
}

fn find_stream_by_name<'a>(
    streams: &'a [ApplicationInfo],
    searched_name: &str,
) -> Option<&'a ApplicationInfo> {
    streams.iter().find(|stream| {
        stream
            .proplist
            .get_str("application.name")
            .is_some_and(|stream_name| stream_name.contains(searched_name))
    })
}

pub struct Routing {
    sink_handler: SinkController,
    source_handler: SourceController,
}

// fn print_controller_info<Controller>(handler: &mut Controller) -> Result<(), Error>
// where
//     Controller: DeviceControl<pulsectl::controllers::types::DeviceInfo>
//         + AppControl<pulsectl::controllers::types::ApplicationInfo>,
// {
//     println!("  Devices");
//     let devices = handler.list_devices()?;
//     for device in &devices {
//         println!(
//             "    [{}] {:?} {:?}",
//             device.index, device.description, device.name
//         );
//     }
//
//     println!("  Defaule device");
//     let device = handler.get_default_device()?;
//     println!(
//         "    [{}] {:?} {:?}",
//         device.index, device.description, device.name
//     );
//
//     println!("  Applications");
//     let applications = handler.list_applications()?;
//     for app in &applications {
//         println!("    [{}] {:?} {:?}", app.index, app.name, app.driver);
//         println!("{}", app.proplist.to_string().unwrap());
//     }
//
//     Ok(())
// }

impl Routing {
    pub fn new() -> Result<Self, Error> {
        // Playback and Out.
        let sink_handler = SinkController::create()?;

        // Recording and In.
        let source_handler = SourceController::create()?;

        Ok(Routing {
            sink_handler,
            source_handler,
        })
    }

    pub fn default_sink(&mut self) -> Result<DeviceInfo, Error> {
        Ok(self.sink_handler.get_default_device()?)
    }

    fn sink_devices(&mut self) -> Result<Vec<DeviceInfo>, Error> {
        Ok(self.sink_handler.list_devices()?)
    }

    fn playback_streams(&mut self) -> Result<Vec<ApplicationInfo>, Error> {
        Ok(self.sink_handler.list_applications()?)
    }

    fn source_devices(&mut self) -> Result<Vec<DeviceInfo>, Error> {
        Ok(self.source_handler.list_devices()?)
    }

    fn record_streams(&mut self) -> Result<Vec<ApplicationInfo>, Error> {
        Ok(self.source_handler.list_applications()?)
    }

    pub fn get_sink_and_monitor_device(
        &mut self,
        sink_name: &str,
    ) -> Result<(DeviceInfo, DeviceInfo), Error> {
        let sink_devices = self.sink_devices()?;
        let sink = find_device_by_name(&sink_devices, sink_name)
            .ok_or_else(|| Error::Local(format!("Can't find sink device {sink_name}")))?;
        let monitor_name = format!("{sink_name}.monitor");
        let source_devices = self.source_devices()?;
        let monitor = find_device_by_name(&source_devices, &monitor_name)
            .ok_or_else(|| Error::Local(format!("Can't find source device {monitor_name}")))?;

        Ok((sink.clone(), monitor.clone()))
    }

    pub fn get_record_and_playback_streams(
        &mut self,
        stream_name: &str,
    ) -> Result<(Option<ApplicationInfo>, Option<ApplicationInfo>), Error> {
        Ok((
            find_stream_by_name(&self.record_streams()?, stream_name).cloned(),
            find_stream_by_name(&self.playback_streams()?, stream_name).cloned(),
        ))
    }

    pub fn set_default_sink_device(&mut self, device: &DeviceInfo) -> Result<(), Error> {
        self.sink_handler
            .set_default_device(device.name.as_ref().unwrap())?
            .then_some(())
            .ok_or_else(|| {
                let msg = format!(
                    "Failed to set default sink to {}",
                    device.name.as_ref().unwrap()
                );
                Error::Local(msg)
            })
    }

    pub fn set_record_input(
        &mut self,
        record_stream: &ApplicationInfo,
        source_device: &DeviceInfo,
    ) -> Result<(), Error> {
        self.source_handler
            .move_app_by_index(record_stream.index, source_device.index)?
            .then_some(())
            .ok_or_else(|| {
                let msg = format!(
                    "Failed to redirect device {} to stream {}",
                    source_device.name.as_ref().unwrap(),
                    record_stream.name.as_ref().unwrap(),
                );
                Error::Local(msg)
            })
    }

    pub fn set_playback_output(
        &mut self,
        playback_stream: &ApplicationInfo,
        sink_device: &DeviceInfo,
    ) -> Result<(), Error> {
        self.sink_handler
            .move_app_by_index(playback_stream.index, sink_device.index)?
            .then_some(())
            .ok_or_else(|| {
                let msg = format!(
                    "Failed to redirect stream {} to device {}",
                    playback_stream.name.as_ref().unwrap(),
                    sink_device.name.as_ref().unwrap(),
                );
                Error::Local(msg)
            })
    }
}
