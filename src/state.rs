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
    /// Periodic measurement started; `self_test_ok` is false if the self test
    /// failed, in which case readings are shown with a warning.
    WarmingUp {
        self_test_ok: bool,
    },
    Data(SensorData),
    /// A sensor step failed; the sensor task has stopped.
    Error {
        step: &'static str,
        detail: heapless::String<96>,
    },
}

pub static CHANNEL: Signal<CriticalSectionRawMutex, SensorState> = Signal::new();

pub static I2C_BUS: StaticCell<I2cBus> = StaticCell::new();
