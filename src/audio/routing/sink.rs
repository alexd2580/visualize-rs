use libpulse_binding::{callbacks::ListResult, context::introspect};

use crate::error::Error;

use super::{
    types::{ApplicationInfo, DeviceInfo},
    Cell, Routing,
};

impl Routing {
    pub fn list_sink_devices(&mut self) -> Result<Vec<DeviceInfo>, Error> {
        let result = Cell::new(Vec::new());
        self.wait_for_operation({
            let result = result.clone();
            self.introspect
                .get_sink_info_list(move |sinks: ListResult<&introspect::SinkInfo>| {
                    if let ListResult::Item(sink) = sinks {
                        result.as_mut_ref().push(sink.into());
                    }
                })
        })?;
        result.into_inner()
    }

    pub fn get_sink_device_by_name(&mut self, name: &str) -> Result<DeviceInfo, Error> {
        let result = Cell::new(None);
        self.wait_for_operation({
            let result = result.clone();
            self.introspect.get_sink_info_by_name(
                name,
                move |sinks: ListResult<&introspect::SinkInfo>| {
                    if let ListResult::Item(sink) = sinks {
                        result.set(Some(sink.into()));
                    }
                },
            )
        })?;
        result
            .into_inner()?
            .ok_or_else(|| Error::Local(format!("Failed to get sink device {}", name)))
    }

    pub fn get_default_sink_device(&mut self) -> Result<DeviceInfo, Error> {
        let server_info = self.get_server_info()?;
        self.get_sink_device_by_name(server_info.default_sink_name.unwrap().as_ref())
    }

    pub fn set_default_sink_device(&mut self, name: &str) -> Result<bool, Error> {
        let result = Cell::new(false);
        let operation = {
            let result = result.clone();
            self.context
                .set_default_sink(name, move |success| result.set(success))
        };
        self.wait_for_operation(operation)?;
        result.into_inner()
    }

    pub fn list_playback_applications(&mut self) -> Result<Vec<ApplicationInfo>, Error> {
        let result = Cell::new(Vec::new());
        self.wait_for_operation({
            let result = result.clone();
            self.introspect.get_sink_input_info_list(
                move |infos: ListResult<&introspect::SinkInputInfo>| {
                    if let ListResult::Item(info) = infos {
                        result.as_mut_ref().push(info.into());
                    }
                },
            )
        })?;
        result.into_inner()
    }

    /// Direct audio producing application to a sink (playback device).
    pub fn set_playback_output(
        &mut self,
        playback_stream: &ApplicationInfo,
        sink_device: &DeviceInfo,
    ) -> Result<bool, Error> {
        let result = Cell::new(false);
        let operation = {
            let result = result.clone();
            self.introspect.move_sink_input_by_index(
                playback_stream.index,
                sink_device.index,
                Some(Box::new(move |res| result.set(res))),
            )
        };
        self.wait_for_operation(operation)?;
        result.into_inner()
    }
}
