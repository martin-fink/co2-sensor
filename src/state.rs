use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use esp_hal::Async;
use esp_hal::i2c::master::I2c;
use scd4x::types::SensorData;
use static_cell::StaticCell;

pub type I2cBus = Mutex<NoopRawMutex, I2c<'static, Async>>;

pub enum SensorState {
    Booting,
    SensorInit,
    SelfTest {
        temp_offset: f32,
        altitude: u16,
        automatic_self_calibration: bool,
    },
    WarmingUp,
    Data(SensorData),
    Error,
}

pub static CHANNEL: Signal<CriticalSectionRawMutex, SensorState> = Signal::new();

pub static I2C_BUS: StaticCell<I2cBus> = StaticCell::new();
