use {
    crate::{
        backend::{self, InputDeviceAccelProfile, InputDeviceId},
        client::{Client, ClientError},
        clientmem::{ClientMem, ClientMemError},
        ifs::wl_seat::WlSeatGlobal,
        leaks::Tracker,
        libinput::consts::{
            AccelProfile, LIBINPUT_CONFIG_ACCEL_PROFILE_ADAPTIVE,
            LIBINPUT_CONFIG_ACCEL_PROFILE_FLAT,
        },
        object::{Object, Version},
        state::{DeviceHandlerData, InputDeviceData},
        utils::errorfmt::ErrorFmt,
        wire::{jay_input::*, JayInputId},
        xkbcommon::XkbCommonError,
    },
    std::rc::Rc,
    thiserror::Error,
};

pub struct JayInput {
    pub id: JayInputId,
    pub client: Rc<Client>,
    pub tracker: Tracker<Self>,
}

impl JayInput {
    pub fn new(id: JayInputId, client: &Rc<Client>) -> Self {
        Self {
            id,
            client: client.clone(),
            tracker: Default::default(),
        }
    }

    fn seat(&self, name: &str) -> Result<Rc<WlSeatGlobal>, JayInputError> {
        for seat in self.client.state.globals.seats.lock().values() {
            if seat.seat_name() == name {
                return Ok(seat.clone());
            }
        }
        Err(JayInputError::SeatDoesNotExist(name.to_string()))
    }

    fn or_error(&self, f: impl FnOnce() -> Result<(), JayInputError>) -> Result<(), JayInputError> {
        if let Err(e) = f() {
            self.send_error(&ErrorFmt(e).to_string());
        }
        Ok(())
    }

    fn send_seat(&self, data: &WlSeatGlobal) {
        self.client.event(Seat {
            self_id: self.id,
            name: data.seat_name(),
            repeat_rate: data.get_rate().0,
            repeat_delay: data.get_rate().1,
            hardware_cursor: data.hardware_cursor() as _,
        });
    }

    fn send_error(&self, error: &str) {
        self.client.event(Error {
            self_id: self.id,
            msg: error,
        });
    }

    fn send_keymap(&self, data: &WlSeatGlobal) {
        let map = data.keymap();
        self.client.event(Keymap {
            self_id: self.id,
            keymap: map.map.clone(),
            keymap_len: (map.map_len - 1) as _,
        });
    }

    fn send_input_device(&self, data: &InputDeviceData) {
        use backend::InputDeviceCapability::*;
        let mut caps = vec![];
        for cap in [
            Keyboard, Pointer, Touch, TabletTool, TabletPad, Gesture, Switch,
        ] {
            if data.data.device.has_capability(cap) {
                caps.push(cap.to_libinput().raw());
            }
        }
        let dev = &data.data.device;
        let accel_profile = dev.accel_profile();
        let left_handed = dev.left_handed();
        let natural_scrolling = dev.natural_scrolling_enabled();
        let tap_enabled = dev.tap_enabled();
        let transform_matrix = dev.transform_matrix();
        self.client.event(InputDevice {
            self_id: self.id,
            seat: data
                .data
                .seat
                .get()
                .as_deref()
                .map(|s| s.seat_name())
                .unwrap_or_default(),
            id: data.id.raw(),
            syspath: data.data.syspath.as_deref().unwrap_or_default(),
            devnode: data.data.devnode.as_deref().unwrap_or_default(),
            name: dev.name().as_str(),
            capabilities: &caps,
            accel_available: accel_profile.is_some() as _,
            accel_profile: match accel_profile {
                None => 0,
                Some(p) => match p {
                    InputDeviceAccelProfile::Flat => LIBINPUT_CONFIG_ACCEL_PROFILE_FLAT.0,
                    InputDeviceAccelProfile::Adaptive => LIBINPUT_CONFIG_ACCEL_PROFILE_ADAPTIVE.0,
                },
            },
            accel_speed: dev.accel_speed().unwrap_or_default(),
            left_handed_available: left_handed.is_some() as _,
            left_handed: left_handed.unwrap_or_default() as _,
            natural_scrolling_available: natural_scrolling.is_some() as _,
            natural_scrolling_enabled: natural_scrolling.unwrap_or_default() as _,
            px_per_wheel_scroll: data.data.px_per_scroll_wheel.get(),
            tap_available: tap_enabled.is_some() as _,
            tap_enabled: tap_enabled.unwrap_or_default() as _,
            tap_drag_enabled: dev.drag_enabled().unwrap_or_default() as _,
            tap_drag_lock_enabled: dev.drag_lock_enabled().unwrap_or_default() as _,
            transform_matrix: transform_matrix
                .as_ref()
                .map(uapi::as_bytes)
                .unwrap_or_default(),
        });
    }

    fn device(&self, id: u32) -> Result<Rc<DeviceHandlerData>, JayInputError> {
        let idh = self.client.state.input_device_handlers.borrow_mut();
        match idh.get(&InputDeviceId::from_raw(id)) {
            None => Err(JayInputError::DeviceDoesNotExist(id)),
            Some(d) => Ok(d.data.clone()),
        }
    }
}

impl JayInputRequestHandler for JayInput {
    type Error = JayInputError;

    fn destroy(&self, _req: Destroy, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.client.remove_obj(self)?;
        Ok(())
    }

    fn get_all(&self, _req: GetAll, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        let state = &self.client.state;
        for seat in state.globals.seats.lock().values() {
            self.send_seat(seat);
        }
        for dev in state.input_device_handlers.borrow().values() {
            self.send_input_device(dev);
        }
        Ok(())
    }

    fn set_repeat_rate(&self, req: SetRepeatRate, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.or_error(|| {
            if req.repeat_rate < 0 {
                return Err(JayInputError::NegativeRepeatRate);
            }
            if req.repeat_delay < 0 {
                return Err(JayInputError::NegativeRepeatDelay);
            }
            let seat = self.seat(req.seat)?;
            seat.set_rate(req.repeat_rate, req.repeat_delay);
            Ok(())
        })
    }

    fn set_keymap(&self, req: SetKeymap, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        let cm = Rc::new(ClientMem::new(req.keymap.raw(), req.keymap_len as _, true)?).offset(0);
        let mut map = vec![];
        cm.read(&mut map)?;
        self.or_error(|| {
            let map = self.client.state.xkb_ctx.keymap_from_str(&map)?;
            let seat = self.seat(req.seat)?;
            seat.set_keymap(&map);
            Ok(())
        })
    }

    fn use_hardware_cursor(
        &self,
        req: UseHardwareCursor,
        _slf: &Rc<Self>,
    ) -> Result<(), Self::Error> {
        self.or_error(|| {
            let seat = self.seat(req.seat)?;
            seat.set_hardware_cursor(req.use_hardware_cursor != 0);
            Ok(())
        })
    }

    fn get_keymap(&self, req: GetKeymap, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.or_error(|| {
            let seat = self.seat(req.seat)?;
            self.send_keymap(&seat);
            Ok(())
        })
    }

    fn set_accel_profile(&self, req: SetAccelProfile, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.or_error(|| {
            let dev = self.device(req.id)?;
            let profile = match AccelProfile(req.profile) {
                LIBINPUT_CONFIG_ACCEL_PROFILE_FLAT => InputDeviceAccelProfile::Flat,
                LIBINPUT_CONFIG_ACCEL_PROFILE_ADAPTIVE => InputDeviceAccelProfile::Adaptive,
                _ => return Err(JayInputError::UnknownAccelerationProfile(req.profile)),
            };
            dev.device.set_accel_profile(profile);
            Ok(())
        })
    }

    fn set_accel_speed(&self, req: SetAccelSpeed, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.or_error(|| {
            let dev = self.device(req.id)?;
            dev.device.set_accel_speed(req.speed);
            Ok(())
        })
    }

    fn set_tap_enabled(&self, req: SetTapEnabled, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.or_error(|| {
            let dev = self.device(req.id)?;
            dev.device.set_tap_enabled(req.enabled != 0);
            Ok(())
        })
    }

    fn set_tap_drag_enabled(
        &self,
        req: SetTapDragEnabled,
        _slf: &Rc<Self>,
    ) -> Result<(), Self::Error> {
        self.or_error(|| {
            let dev = self.device(req.id)?;
            dev.device.set_drag_enabled(req.enabled != 0);
            Ok(())
        })
    }

    fn set_tap_drag_lock_enabled(
        &self,
        req: SetTapDragLockEnabled,
        _slf: &Rc<Self>,
    ) -> Result<(), Self::Error> {
        self.or_error(|| {
            let dev = self.device(req.id)?;
            dev.device.set_drag_lock_enabled(req.enabled != 0);
            Ok(())
        })
    }

    fn set_left_handed(&self, req: SetLeftHanded, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.or_error(|| {
            let dev = self.device(req.id)?;
            dev.device.set_left_handed(req.enabled != 0);
            Ok(())
        })
    }

    fn set_natural_scrolling(
        &self,
        req: SetNaturalScrolling,
        _slf: &Rc<Self>,
    ) -> Result<(), Self::Error> {
        self.or_error(|| {
            let dev = self.device(req.id)?;
            dev.device.set_natural_scrolling_enabled(req.enabled != 0);
            Ok(())
        })
    }

    fn set_px_per_wheel_scroll(
        &self,
        req: SetPxPerWheelScroll,
        _slf: &Rc<Self>,
    ) -> Result<(), Self::Error> {
        self.or_error(|| {
            let dev = self.device(req.id)?;
            dev.px_per_scroll_wheel.set(req.px);
            Ok(())
        })
    }

    fn set_transform_matrix(
        &self,
        req: SetTransformMatrix,
        _slf: &Rc<Self>,
    ) -> Result<(), Self::Error> {
        self.or_error(|| {
            let dev = self.device(req.id)?;
            dev.device
                .set_transform_matrix([[req.m11, req.m12], [req.m21, req.m22]]);
            Ok(())
        })
    }

    fn set_cursor_size(&self, req: SetCursorSize, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.or_error(|| {
            let seat = self.seat(req.seat)?;
            seat.set_cursor_size(req.size);
            Ok(())
        })
    }

    fn attach(&self, req: Attach, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.or_error(|| {
            let seat = self.seat(req.seat)?;
            let dev = self.device(req.id)?;
            dev.seat.set(Some(seat));
            Ok(())
        })
    }

    fn detach(&self, req: Detach, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.or_error(|| {
            let dev = self.device(req.id)?;
            dev.seat.set(None);
            Ok(())
        })
    }

    fn get_seat(&self, req: GetSeat, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.or_error(|| {
            let seat = self.seat(req.name)?;
            self.send_seat(&seat);
            for dev in self.client.state.input_device_handlers.borrow().values() {
                if let Some(attached) = dev.data.seat.get() {
                    if attached.id() == seat.id() {
                        self.send_input_device(dev);
                    }
                }
            }
            Ok(())
        })
    }

    fn get_device(&self, req: GetDevice, _slf: &Rc<Self>) -> Result<(), Self::Error> {
        self.or_error(|| {
            match self
                .client
                .state
                .input_device_handlers
                .borrow()
                .get(&InputDeviceId::from_raw(req.id))
            {
                None => Err(JayInputError::DeviceDoesNotExist(req.id)),
                Some(d) => {
                    self.send_input_device(d);
                    Ok(())
                }
            }
        })
    }
}

object_base! {
    self = JayInput;
    version = Version(1);
}

impl Object for JayInput {}

simple_add_obj!(JayInput);

#[derive(Debug, Error)]
pub enum JayInputError {
    #[error(transparent)]
    ClientError(Box<ClientError>),
    #[error("There is no seat called {0}")]
    SeatDoesNotExist(String),
    #[error("There is no device with id {0}")]
    DeviceDoesNotExist(u32),
    #[error("There is no acceleration profile with id {0}")]
    UnknownAccelerationProfile(i32),
    #[error("Repeat rate must not be negative")]
    NegativeRepeatRate,
    #[error("Repeat delay must not be negative")]
    NegativeRepeatDelay,
    #[error("Could not access client memory")]
    ClientMemError(#[from] ClientMemError),
    #[error("Could not parse keymap")]
    XkbCommonError(#[from] XkbCommonError),
}
efrom!(JayInputError, ClientError);