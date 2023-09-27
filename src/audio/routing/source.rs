use libpulse_binding::{callbacks::ListResult, context::introspect};

use crate::error::Error;

use super::{
    types::{ApplicationInfo, DeviceInfo},
    Cell, Routing,
};

impl Routing {
    pub fn list_source_devices(&mut self) -> Result<Vec<DeviceInfo>, Error> {
        let result = Cell::new(Vec::new());
        self.wait_for_operation({
            let result = result.clone();
            self.introspect.get_source_info_list(
                move |sources: ListResult<&introspect::SourceInfo>| {
                    if let ListResult::Item(source) = sources {
                        result.as_mut_ref().push(source.into());
                    }
                },
            )
        })?;
        result.into_inner()
    }

    pub fn get_source_device_by_name(&mut self, name: &str) -> Result<DeviceInfo, Error> {
        let result = Cell::new(None);
        self.wait_for_operation({
            let result = result.clone();
            self.introspect.get_source_info_by_name(
                name,
                move |sources: ListResult<&introspect::SourceInfo>| {
                    if let ListResult::Item(source) = sources {
                        result.set(Some(source.into()));
                    }
                },
            )
        })?;
        result
            .into_inner()?
            .ok_or_else(|| Error::Local(format!("Failed to get source device {}", name)))
    }

    pub fn get_default_source_device(&mut self) -> Result<DeviceInfo, Error> {
        let server_info = self.get_server_info()?;
        self.get_source_device_by_name(server_info.default_source_name.unwrap().as_ref())
    }

    pub fn set_default_source_device(&mut self, name: &str) -> Result<bool, Error> {
        let result = Cell::new(false);
        let operation = {
            let result = result.clone();
            self.context
                .set_default_source(name, move |success| result.set(success))
        };
        self.wait_for_operation(operation)?;
        result.into_inner()
    }

    pub fn list_record_applications(&mut self) -> Result<Vec<ApplicationInfo>, Error> {
        let result = Cell::new(Vec::new());
        self.wait_for_operation({
            let result = result.clone();
            self.introspect.get_source_output_info_list(
                move |infos: ListResult<&introspect::SourceOutputInfo>| {
                    if let ListResult::Item(info) = infos {
                        result.as_mut_ref().push(info.into());
                    }
                },
            )
        })?;
        result.into_inner()
    }

    /// Link source device with consuming stream.
    pub fn set_record_input(
        &mut self,
        record_stream: &ApplicationInfo,
        source_device: &DeviceInfo,
    ) -> Result<bool, Error> {
        let result = Cell::new(false);
        let operation = {
            let result = result.clone();
            self.introspect.move_source_output_by_index(
                record_stream.index,
                source_device.index,
                Some(Box::new(move |res| result.set(res))),
            )
        };
        self.wait_for_operation(operation)?;
        result.into_inner()
    }
}
