use crate::backend::{KeyboardId, MouseId};
use crate::ifs::wl_seat::WlSeatGlobal;
use crate::state::DeviceHandlerData;
use crate::tree::ContainerSplit;
use crate::utils::copyhashmap::CopyHashMap;
use crate::utils::debug_fn::debug_fn;
use crate::utils::stack::Stack;
use crate::xkbcommon::XkbKeymap;
use crate::{ErrorFmt, NumCell, State};
use bincode::error::DecodeError;
use i4config::_private::bincode_ops;
use i4config::_private::ipc::{ClientMessage, Response, ServerMessage};
use i4config::keyboard::keymap::Keymap;
use i4config::keyboard::mods::Modifiers;
use i4config::keyboard::syms::KeySym;
use i4config::{Axis, Direction, InputDevice, Keyboard, LogLevel, Mouse, Seat};
use libloading::Library;
use log::Level;
use std::cell::Cell;
use std::rc::Rc;
use thiserror::Error;

pub(super) struct ConfigProxyHandler {
    pub client_data: Cell<*const u8>,
    pub dropped: Cell<bool>,
    pub _lib: Option<Library>,
    pub _version: u32,
    pub unref: unsafe extern "C" fn(data: *const u8),
    pub handle_msg: unsafe extern "C" fn(data: *const u8, msg: *const u8, size: usize),
    pub state: Rc<State>,
    pub next_id: NumCell<u64>,
    pub keymaps: CopyHashMap<Keymap, Rc<XkbKeymap>>,
    pub bufs: Stack<Vec<u8>>,
}

impl ConfigProxyHandler {
    pub fn send(&self, msg: &ServerMessage) {
        let mut buf = self.bufs.pop().unwrap_or_default();
        buf.clear();
        bincode::encode_into_std_write(msg, &mut buf, bincode_ops()).unwrap();
        unsafe {
            (self.handle_msg)(self.client_data.get(), buf.as_ptr(), buf.len());
        }
        self.bufs.push(buf);
    }

    fn id(&self) -> u64 {
        self.next_id.fetch_add(1)
    }

    fn handle_log_request(
        &self,
        level: LogLevel,
        msg: &str,
        file: Option<&str>,
        line: Option<u32>,
    ) -> Result<(), LogError> {
        let level = match level {
            LogLevel::Error => Level::Error,
            LogLevel::Warn => Level::Warn,
            LogLevel::Info => Level::Info,
            LogLevel::Debug => Level::Debug,
            LogLevel::Trace => Level::Trace,
        };
        let debug = debug_fn(|fmt| {
            if let Some(file) = file {
                write!(fmt, "{}", file)?;
                if let Some(line) = line {
                    write!(fmt, ":{}", line)?;
                }
                write!(fmt, ": ")?;
            }
            write!(fmt, "{}", msg)?;
            Ok(())
        });
        log::log!(level, "{:?}", debug);
        Ok(())
    }

    fn handle_create_seat(&self, name: &str) -> Result<(), CreateSeatError> {
        let global_name = self.state.globals.name();
        let seat = WlSeatGlobal::new(global_name, name, &self.state);
        self.state.globals.add_global(&self.state, &seat);
        self.send(&ServerMessage::Response {
            response: Response::CreateSeat {
                seat: Seat(seat.id().raw() as _),
            },
        });
        Ok(())
    }

    fn handle_parse_keymap(&self, keymap: &str) -> Result<(), ParseKeymapError> {
        let (keymap, res) = match self.state.xkb_ctx.keymap_from_str(keymap) {
            Ok(keymap) => {
                let id = Keymap(self.id());
                self.keymaps.set(id, keymap);
                (id, Ok(()))
            }
            _ => (Keymap::INVALID, Err(ParseKeymapError::ParsingFailed)),
        };
        self.send(&ServerMessage::Response {
            response: Response::ParseKeymap { keymap },
        });
        res
    }

    fn handle_set_keymap(&self, seat: Seat, keymap: Keymap) -> Result<(), SeatSetKeymapError> {
        let seat = self.get_seat(seat)?;
        let keymap = if keymap.is_invalid() {
            self.state.default_keymap.clone()
        } else {
            self.get_keymap(keymap)?
        };
        seat.set_keymap(&keymap);
        Ok(())
    }

    fn handle_focus(&self, seat: Seat, direction: Direction) -> Result<(), FocusError> {
        let seat = self.get_seat(seat)?;
        seat.move_focus(direction);
        Ok(())
    }

    fn handle_get_repeat_rate(&self, seat: Seat) -> Result<(), SeatGetRepeatRateError> {
        let seat = self.get_seat(seat)?;
        let (rate, delay) = seat.get_rate();
        self.send(&ServerMessage::Response {
            response: Response::GetRepeatRate { rate, delay },
        });
        Ok(())
    }

    fn handle_set_repeat_rate(
        &self,
        seat: Seat,
        rate: i32,
        delay: i32,
    ) -> Result<(), SeatSetRepeatRateError> {
        let seat = self.get_seat(seat)?;
        if rate < 0 {
            return Err(SeatSetRepeatRateError::NegativeRate);
        }
        if delay < 0 {
            return Err(SeatSetRepeatRateError::NegativeDelay);
        }
        seat.set_rate(rate, delay);
        Ok(())
    }

    fn get_device_handler_data(
        &self,
        device: InputDevice,
    ) -> Result<Rc<DeviceHandlerData>, CphError> {
        let data = match device {
            InputDevice::Keyboard(kb) => self
                .state
                .kb_handlers
                .borrow_mut()
                .get(&KeyboardId::from_raw(kb.0 as _))
                .map(|d| d.data.clone()),
            InputDevice::Mouse(mouse) => self
                .state
                .mouse_handlers
                .borrow_mut()
                .get(&MouseId::from_raw(mouse.0 as _))
                .map(|d| d.data.clone()),
        };
        match data {
            Some(d) => Ok(d),
            _ => Err(CphError::DeviceDoesNotExist(device)),
        }
    }

    fn get_seat(&self, seat: Seat) -> Result<Rc<WlSeatGlobal>, CphError> {
        let seats = self.state.globals.seats.lock();
        for seat_global in seats.values() {
            if seat_global.id().raw() == seat.0 as _ {
                return Ok(seat_global.clone());
            }
        }
        Err(CphError::SeatDoesNotExist(seat))
    }

    fn get_keymap(&self, keymap: Keymap) -> Result<Rc<XkbKeymap>, CphError> {
        match self.keymaps.get(&keymap) {
            Some(k) => Ok(k),
            None => Err(CphError::KeymapDoesNotExist(keymap)),
        }
    }

    fn handle_set_seat(&self, device: InputDevice, seat: Seat) -> Result<(), SetSeatError> {
        let seat = if seat.is_invalid() {
            None
        } else {
            Some(self.get_seat(seat)?)
        };
        let dev = self.get_device_handler_data(device)?;
        dev.seat.set(seat);
        Ok(())
    }

    fn handle_get_split(&self, seat: Seat) -> Result<(), GetSplitError> {
        let seat = self.get_seat(seat)?;
        self.send(&ServerMessage::Response {
            response: Response::GetSplit {
                axis: seat
                    .get_split()
                    .unwrap_or(ContainerSplit::Horizontal)
                    .into(),
            },
        });
        Ok(())
    }

    fn handle_set_split(&self, seat: Seat, axis: Axis) -> Result<(), SetSplitError> {
        let seat = self.get_seat(seat)?;
        seat.set_split(axis.into());
        Ok(())
    }

    fn handle_add_shortcut(
        &self,
        seat: Seat,
        mods: Modifiers,
        sym: KeySym,
    ) -> Result<(), AddShortcutError> {
        let seat = self.get_seat(seat)?;
        seat.add_shortcut(mods, sym);
        Ok(())
    }

    fn handle_get_input_devices(&self) -> Result<(), GetInputDevicesError> {
        let mut res = vec![];
        {
            let devs = self.state.kb_handlers.borrow_mut();
            for dev in devs.values() {
                res.push(InputDevice::Keyboard(Keyboard(dev.id.raw() as _)));
            }
        }
        {
            let devs = self.state.mouse_handlers.borrow_mut();
            for dev in devs.values() {
                res.push(InputDevice::Mouse(Mouse(dev.id.raw() as _)));
            }
        }
        self.send(&ServerMessage::Response {
            response: Response::GetInputDevices { devices: res },
        });
        Ok(())
    }

    fn handle_get_seats(&self) -> Result<(), GetSeatsError> {
        let seats = {
            let seats = self.state.globals.seats.lock();
            seats
                .values()
                .map(|seat| Seat::from_raw(seat.id().raw() as _))
                .collect()
        };
        self.send(&ServerMessage::Response {
            response: Response::GetSeats { seats },
        });
        Ok(())
    }

    fn handle_run(&self, prog: &str, args: Vec<String>, env: Vec<(String, String)>) -> Result<(), RunError> {
        let forker = match self.state.forker.get() {
            Some(f) => f,
            _ => return Err(RunError::NoForker),
        };
        forker.spawn(prog.to_string(), args, env);
        Ok(())
    }

    pub fn handle_request(&self, msg: &[u8]) {
        if let Err(e) = self.handle_request_(msg) {
            log::error!("Could not handle client request: {}", ErrorFmt(e));
        }
    }

    fn handle_request_(&self, msg: &[u8]) -> Result<(), CphError> {
        let (request, _) = match bincode::decode_from_slice::<ClientMessage, _>(msg, bincode_ops())
        {
            Ok(msg) => msg,
            Err(e) => return Err(CphError::ParsingFailed(e)),
        };
        match request {
            ClientMessage::Log {
                level,
                msg,
                file,
                line,
            } => self.handle_log_request(level, msg, file, line)?,
            ClientMessage::CreateSeat { name } => self.handle_create_seat(name)?,
            ClientMessage::ParseKeymap { keymap } => self.handle_parse_keymap(keymap)?,
            ClientMessage::SeatSetKeymap { seat, keymap } => {
                self.handle_set_keymap(seat, keymap)?
            }
            ClientMessage::SeatGetRepeatRate { seat } => self.handle_get_repeat_rate(seat)?,
            ClientMessage::SeatSetRepeatRate { seat, rate, delay } => {
                self.handle_set_repeat_rate(seat, rate, delay)?
            }
            ClientMessage::SetSeat { device, seat } => self.handle_set_seat(device, seat)?,
            ClientMessage::GetSplit { seat } => self.handle_get_split(seat)?,
            ClientMessage::SetSplit { seat, axis } => self.handle_set_split(seat, axis)?,
            ClientMessage::AddShortcut { seat, mods, sym } => {
                self.handle_add_shortcut(seat, mods, sym)?
            }
            ClientMessage::RemoveShortcut { .. } => {}
            ClientMessage::Focus { seat, direction } => self.handle_focus(seat, direction)?,
            ClientMessage::Move { seat, direction } => {}
            ClientMessage::GetInputDevices => self.handle_get_input_devices()?,
            ClientMessage::GetSeats => self.handle_get_seats()?,
            ClientMessage::RemoveSeat { .. } => {}
            ClientMessage::Run { prog, args, env } => self.handle_run(prog, args, env)?,
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
enum CphError {
    #[error("Could not process a `log` request")]
    LogError(#[from] LogError),
    #[error("Could not process a `create_seat` request")]
    CreateSeatError(#[from] CreateSeatError),
    #[error("Could not process a `parse_keymap` request")]
    ParseKeymapError(#[from] ParseKeymapError),
    #[error("Could not process a `set_seat` request")]
    SetSeatError(#[from] SetSeatError),
    #[error("Could not process a `add_shortcut` request")]
    AddShortcutError(#[from] AddShortcutError),
    #[error("Could not process a `get_input_devices` request")]
    GetInputDevicesError(#[from] GetInputDevicesError),
    #[error("Could not process a `get_seats` request")]
    GetSeatsError(#[from] GetSeatsError),
    #[error("Could not process a `set_keymap` request")]
    SeatSetKeymapError(#[from] SeatSetKeymapError),
    #[error("Could not process a `get_repeat_rate` request")]
    SeatGetRepeatRateError(#[from] SeatGetRepeatRateError),
    #[error("Could not process a `set_repeat_rate` request")]
    SeatSetRepeatRateError(#[from] SeatSetRepeatRateError),
    #[error("Could not process a `focus` request")]
    FocusError(#[from] FocusError),
    #[error("Could not process a `set_split` request")]
    SetSplitError(#[from] SetSplitError),
    #[error("Could not process a `get_split` request")]
    GetSplitError(#[from] GetSplitError),
    #[error("Could not process a `run` request")]
    RunError(#[from] RunError),
    #[error("Device {0:?} does not exist")]
    DeviceDoesNotExist(InputDevice),
    #[error("Device {0:?} does not exist")]
    KeymapDoesNotExist(Keymap),
    #[error("Seat {0:?} does not exist")]
    SeatDoesNotExist(Seat),
    #[error("Seat {0:?} does not exist")]
    UnexpectedMessage(String),
    #[error("Could not parse the message")]
    ParsingFailed(#[source] DecodeError),
}

#[derive(Debug, Error)]
enum LogError {}

#[derive(Debug, Error)]
enum CreateSeatError {}

#[derive(Debug, Error)]
enum ParseKeymapError {
    #[error("Parsing failed")]
    ParsingFailed,
}

#[derive(Debug, Error)]
enum SetSeatError {
    #[error(transparent)]
    CphError(#[from] Box<CphError>),
}
efrom!(SetSeatError, CphError);

#[derive(Debug, Error)]
enum AddShortcutError {
    #[error(transparent)]
    CphError(#[from] Box<CphError>),
}
efrom!(AddShortcutError, CphError);

#[derive(Debug, Error)]
enum GetInputDevicesError {}

#[derive(Debug, Error)]
enum GetSeatsError {}

#[derive(Debug, Error)]
enum SeatSetKeymapError {
    #[error(transparent)]
    CphError(#[from] Box<CphError>),
}
efrom!(SeatSetKeymapError, CphError);

#[derive(Debug, Error)]
enum SeatSetRepeatRateError {
    #[error(transparent)]
    CphError(#[from] Box<CphError>),
    #[error("Rate is negative")]
    NegativeRate,
    #[error("Delay is negative")]
    NegativeDelay,
}
efrom!(SeatSetRepeatRateError, CphError);

#[derive(Debug, Error)]
enum SeatGetRepeatRateError {
    #[error(transparent)]
    CphError(#[from] Box<CphError>),
}
efrom!(SeatGetRepeatRateError, CphError);

#[derive(Debug, Error)]
enum FocusError {
    #[error(transparent)]
    CphError(#[from] Box<CphError>),
}
efrom!(FocusError, CphError);

#[derive(Debug, Error)]
enum SetSplitError {
    #[error(transparent)]
    CphError(#[from] Box<CphError>),
}
efrom!(SetSplitError, CphError);

#[derive(Debug, Error)]
enum GetSplitError {
    #[error(transparent)]
    CphError(#[from] Box<CphError>),
}
efrom!(GetSplitError, CphError);

#[derive(Debug, Error)]
enum RunError {
    #[error("The ol' forker is not available")]
    NoForker,
}