use crate::config::*;
use crate::state::{CHANNEL, SensorState};
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

    CHANNEL.signal(SensorState::SensorInit);
    let _ = sensor.stop_periodic_measurement().await;
    Timer::after(Duration::from_millis(500)).await;
    sensor.reinit().await.unwrap();
    Timer::after(Duration::from_millis(20)).await;

    sensor.set_altitude(ALTITUDE).await.unwrap();
    sensor
        .set_automatic_self_calibration(AUTOMATIC_SELF_CALIBRATION)
        .await
        .unwrap();
    if let Some(temp_offset) = TEMP_OFFSET {
        sensor.set_temperature_offset(temp_offset).await.unwrap();
    }

    let automatic_self_calibration = sensor.automatic_self_calibration().await.unwrap_or(false);
    let altitude = sensor.altitude().await.unwrap_or(0);
    let temp_offset = sensor.temperature_offset().await.unwrap_or(0.0);
    info!("asc={automatic_self_calibration}, altitude={altitude}, temp_offset={temp_offset}");
    CHANNEL.signal(SensorState::SelfTest {
        altitude,
        automatic_self_calibration,
        temp_offset,
    });

    info!("serial: {:?}", sensor.serial_number().await);
    let ok = sensor.self_test_is_ok().await.unwrap_or(false);
    info!("self test ok: {:?}", ok);
    if !ok {
        panic!("sensor test failed");
    }

    CHANNEL.signal(SensorState::WarmingUp);
    sensor.start_periodic_measurement().await.unwrap();

    loop {
        Timer::after(Duration::from_secs(5)).await;

        if sensor.data_ready_status().await.unwrap_or(false) {
            CHANNEL.signal(SensorState::Data(sensor.measurement().await.unwrap()));
        }
    }
}
