use libpulse_binding::{
    context::{self, introspect::Introspector, Context, FlagSet},
    mainloop::standard::{IterateResult, Mainloop},
    operation::{self, Operation},
    proplist::{properties, Proplist},
};

use crate::{cell::Cell, error::Error};

use self::types::{ApplicationInfo, ServerInfo};

mod sink;
mod source;
pub mod types;

pub struct Routing {
    pub mainloop: Mainloop,
    pub context: Context,
    pub introspect: Introspector,
}

impl Drop for Routing {
    fn drop(&mut self) {
        self.context.disconnect();
        self.mainloop.quit(libpulse_binding::def::Retval(0));
    }
}

impl Routing {
    pub fn find_stream_by_name<'a>(
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

    fn poll_mainloop(mainloop: &mut Mainloop) -> Result<bool, Error> {
        match mainloop.iterate(false) {
            IterateResult::Err(e) => Err(Error::Libpulse(e)),
            IterateResult::Success(_) => Ok(true),
            IterateResult::Quit(_) => Ok(false),
        }
    }

    fn check_context(context: &Context) -> Result<bool, Error> {
        match context.get_state() {
            context::State::Ready => Ok(true),
            context::State::Failed | context::State::Terminated => Err(Error::Local(
                "Context state is failed/terminated".to_string(),
            )),
            _ => Ok(false),
        }
    }

    fn check_operation<G: ?Sized>(operation: &Operation<G>) -> Result<bool, Error> {
        match operation.get_state() {
            operation::State::Done => Ok(true),
            operation::State::Running => Ok(false),
            operation::State::Cancelled => Err(Error::Local(
                "Operation cancelled without an error".to_string(),
            )),
        }
    }

    pub fn new() -> Result<Self, Error> {
        let str = format!("{}=\"Visualize-rs\"", properties::APPLICATION_NAME);
        let proplist = Proplist::new_from_string(&str).unwrap();

        let mut mainloop = Mainloop::new().unwrap();
        // let mainloop = Rc::new(RefCell::new(mainloop));

        let mut context = Context::new_with_proplist(&mainloop, "MainConn", &proplist).unwrap();
        // let context = Rc::new(RefCell::new(context));

        context
            .connect(None, FlagSet::NOFLAGS, None)
            .expect("Failed to connect context");

        loop {
            if !Self::poll_mainloop(&mut mainloop)? {
                panic!("Libpulse wants to quit?");
            }
            if Self::check_context(&context)? {
                break;
            }
        }

        let introspect = context.introspect();
        Ok(Routing {
            mainloop,
            context,
            introspect,
        })
    }

    pub fn wait_for_operation<G: ?Sized>(&mut self, operation: Operation<G>) -> Result<(), Error> {
        loop {
            if !Self::poll_mainloop(&mut self.mainloop)? {
                panic!("Libpulse wants to quit?");
            }
            if Self::check_operation(&operation)? {
                break;
            }
        }
        Ok(())
    }

    pub fn get_server_info(&mut self) -> Result<ServerInfo, Error> {
        let result: Cell<Option<ServerInfo>> = Cell::new(None);
        self.wait_for_operation({
            let result = result.clone();
            self.introspect
                .get_server_info(move |server_info| result.set(Some(server_info.into())))
        })?;
        result
            .into_inner()?
            .ok_or_else(|| Error::Local("Failed to get pulse server info".to_string()))
    }

    #[allow(dead_code)]
    pub fn print(&mut self) -> Result<(), Error> {
        println!("Server info {:?}", self.get_server_info()?);
        println!("Sinks");
        println!("  Devices");
        let devices = self.list_sink_devices()?;
        for device in &devices {
            println!(
                "    [{}] {:?} {:?}",
                device.index, device.description, device.name
            );
        }
        println!("  Default device");
        let device = self.get_default_sink_device()?;
        println!(
            "    [{}] {:?} {:?}",
            device.index, device.description, device.name
        );
        println!("  Applications");
        let applications = self.list_playback_applications()?;
        for app in &applications {
            println!("    [{}] {:?} {:?}", app.index, app.name, app.driver);
            println!("{}", app.proplist.to_string().unwrap());
        }

        println!("Sources");
        println!("  Devices");
        let devices = self.list_source_devices()?;
        for device in &devices {
            println!(
                "    [{}] {:?} {:?}",
                device.index, device.description, device.name
            );
        }
        println!("  Default device");
        let device = self.get_default_source_device()?;
        println!(
            "    [{}] {:?} {:?}",
            device.index, device.description, device.name
        );
        println!("  Applications");
        let applications = self.list_record_applications()?;
        for app in &applications {
            println!("    [{}] {:?} {:?}", app.index, app.name, app.driver);
            println!("{}", app.proplist.to_string().unwrap());
        }

        Ok(())
    }
    //
    // pub fn default_sink(&mut self) -> Result<DeviceInfo, Error> {
    //     Ok(self.sink_handler.get_default_device()?)
    // }
    //
    // fn sink_devices(&mut self) -> Result<Vec<DeviceInfo>, Error> {
    //     Ok(self.sink_handler.list_devices()?)
    // }
    //
    // fn playback_streams(&mut self) -> Result<Vec<ApplicationInfo>, Error> {
    //     Ok(self.sink_handler.list_applications()?)
    // }
    //
    // fn source_devices(&mut self) -> Result<Vec<DeviceInfo>, Error> {
    //     Ok(self.source_handler.list_devices()?)
    // }
    //
    // fn record_streams(&mut self) -> Result<Vec<ApplicationInfo>, Error> {
    //     Ok(self.source_handler.list_applications()?)
    // }
    //
    //
    // pub fn set_default_sink(&mut self, device: &DeviceInfo) -> Result<(), Error> {
    //     self.sink_handler
    //         .set_default_device(device.name.as_ref().unwrap())?
    //         .then_some(())
    //         .ok_or_else(|| {
    //             let msg = format!(
    //                 "Failed to set default sink to {}",
    //                 device.name.as_ref().unwrap()
    //             );
    //             Error::Local(msg)
    //         })
    // }
    //
    //
}
