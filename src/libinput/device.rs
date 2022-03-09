use crate::libinput::sys::{
    libinput_device, libinput_device_set_user_data, libinput_device_unref,
    libinput_path_remove_device,
};
use crate::libinput::LibInput;
use std::marker::PhantomData;
use std::rc::Rc;

pub struct LibInputDevice<'a> {
    pub(super) dev: *mut libinput_device,
    pub(super) _phantom: PhantomData<&'a ()>,
}

pub struct RegisteredDevice {
    pub(super) li: Rc<LibInput>,
    pub(super) dev: *mut libinput_device,
}

impl<'a> LibInputDevice<'a> {
    pub fn set_slot(&self, slot: usize) {
        self.set_slot_(slot + 1)
    }

    pub fn unset_slot(&self) {
        self.set_slot_(0)
    }

    fn set_slot_(&self, slot: usize) {
        unsafe {
            libinput_device_set_user_data(self.dev, slot as _);
        }
    }
}

impl RegisteredDevice {
    pub fn device(&self) -> LibInputDevice {
        LibInputDevice {
            dev: self.dev,
            _phantom: Default::default(),
        }
    }
}

impl Drop for RegisteredDevice {
    fn drop(&mut self) {
        unsafe {
            libinput_path_remove_device(self.dev);
            libinput_device_unref(self.dev);
        }
    }
}
