use std::fmt::Display;

use log::{debug, error};

use crate::{error::Error, utils::exec_command};

pub struct VirtualSink {
    pub name: String,
    id: String,
}

impl Display for VirtualSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] '{}'", self.id, self.name)
    }
}

impl VirtualSink {
    pub fn new(name: String) -> Result<Self, Error> {
        debug!("Creating virtual sink: '{name}'");
        let sink_name_arg =
            format!("sink_name=\"{name}\" sink_properties=device.description=\"{name}\"");
        let mut id = exec_command(&["pactl", "load-module", "module-null-sink", &sink_name_arg])?;
        id = id.trim_end().to_owned();
        let virtual_sink = VirtualSink { name, id };
        debug!("Virtual sink {virtual_sink} created");
        Ok(virtual_sink)
    }
}

impl Drop for VirtualSink {
    fn drop(&mut self) {
        debug!("Destroying virtual sink {self}");
        let result = exec_command(&["pactl", "unload-module", &self.id]);
        if let Err(error) = result {
            error!("Failed to unload virtual sink {self}: {error}");
        }
    }
}
