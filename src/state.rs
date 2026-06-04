use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU16, AtomicU32, Ordering};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use esp_hal::Async;
use esp_hal::i2c::master::I2c;
use scd4x::types::SensorData;
use static_cell::StaticCell;

pub type I2cBus = Mutex<NoopRawMutex, I2c<'static, Async>>;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Booting = 0,
    SensorInit = 1,
    SelfTest = 2,
    WarmingUp = 3,
    Ready = 4,
    Error = 5,
}

impl State {
    pub const fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Booting,
            1 => Self::SensorInit,
            2 => Self::SelfTest,
            3 => Self::WarmingUp,
            4 => Self::Ready,
            5 => Self::Error,
            _ => Self::Error, // fallback for corrupted/unknown values
        }
    }
}

pub struct AtomicState {
    inner: AtomicU8,
    pub altitude: AtomicU16,
    pub automatic_self_calibration: AtomicBool,
    temp_offset: AtomicU32,
}

impl AtomicState {
    pub const fn new(state: State) -> Self {
        Self {
            inner: AtomicU8::new(state as u8),
            altitude: AtomicU16::new(0),
            automatic_self_calibration: AtomicBool::new(false),
            temp_offset: AtomicU32::new(0),
        }
    }

    pub fn get_state(&self) -> State {
        State::from_u8(self.inner.load(Ordering::Relaxed))
    }

    pub fn set_state(&self, state: State) {
        self.inner.store(state as u8, Ordering::Relaxed);
    }

    pub fn set_temp_offset(&self, offset: f32) {
        self.temp_offset.store(offset as u32, Ordering::Relaxed);
    }

    pub fn get_temp_offset(&self) -> f32 {
        self.temp_offset.load(Ordering::Relaxed) as f32
    }
}

pub static STATE: AtomicState = AtomicState::new(State::Booting);
pub static MEASUREMENT: Signal<CriticalSectionRawMutex, SensorData> = Signal::new();

pub static I2C_BUS: StaticCell<I2cBus> = StaticCell::new();
