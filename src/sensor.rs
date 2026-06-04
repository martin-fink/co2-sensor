use core::sync::atomic::Ordering;

use crate::config::*;
use crate::state::{MEASUREMENT, STATE, State};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::{Delay, Duration, Timer};
use esp_hal::Async;
use esp_hal::i2c::master::I2c;
use log::info;
use scd4x::Scd4xAsync;

#[embassy_executor::task]
pub async fn sensor_task(i2c: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>) -> ! {
    let mut sensor = Scd4xAsync::new(i2c, Delay);

    STATE.set_state(State::SensorInit);
    let _ = sensor.stop_periodic_measurement().await;
    Timer::after(Duration::from_millis(500)).await;
    sensor.reinit().await.unwrap();
    Timer::after(Duration::from_millis(20)).await;

    sensor.set_altitude(ALTITUDE).await.unwrap();
    if let Some(asc) = AUTOMATIC_SELF_CALIBRATION {
        sensor.set_automatic_self_calibration(asc).await.unwrap();
    }
    if let Some(temp_offset) = TEMP_OFFSET {
        sensor.set_temperature_offset(temp_offset).await.unwrap();
    }

    let asc_enabled = sensor.automatic_self_calibration().await.unwrap_or(false);
    let altitude = sensor.altitude().await.unwrap_or(0);
    let temp_offset = sensor.temperature_offset().await.unwrap_or(0.0);
    info!("asc={asc_enabled}, altitude={altitude}, temp_offset={temp_offset}");
    STATE.altitude.store(altitude, Ordering::Relaxed);
    STATE
        .automatic_self_calibration
        .store(asc_enabled, Ordering::Relaxed);
    STATE.set_temp_offset(temp_offset);

    STATE.set_state(State::SelfTest);
    info!("serial: {:?}", sensor.serial_number().await);
    let ok = sensor.self_test_is_ok().await.unwrap_or(false);
    info!("self test ok: {:?}", ok);
    if !ok {
        STATE.set_state(State::Error);
        panic!("sensor test failed");
    }

    STATE.set_state(State::WarmingUp);
    sensor.start_periodic_measurement().await.unwrap();

    loop {
        Timer::after(Duration::from_secs(5)).await;
        STATE.set_state(State::Ready);

        if sensor.data_ready_status().await.unwrap_or(false) {
            MEASUREMENT.signal(sensor.measurement().await.unwrap());
        }
    }
}
