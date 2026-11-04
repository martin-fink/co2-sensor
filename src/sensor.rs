use crate::config::*;
use crate::state::{CHANNEL, SensorState};
use core::fmt::Write as _;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::{Delay, Duration, Timer};
use esp_hal::Async;
use esp_hal::i2c::master::I2c;
use log::{error, info, warn};
use scd4x::Scd4xAsync;

/// Reports a failed step on the display and parks the sensor task for good.
async fn fail(step: &'static str, detail: impl core::fmt::Display) -> ! {
    error!("{step} failed: {detail}");
    let mut buf = heapless::String::new();
    // Overflow only truncates the message, which is fine for the display.
    let _ = write!(buf, "{detail}");
    CHANNEL.signal(SensorState::Error { step, detail: buf });
    core::future::pending().await
}

/// Unwraps a sensor result, or reports the error via [`fail`].
macro_rules! check {
    ($step:literal, $result:expr) => {
        match $result {
            Ok(v) => v,
            Err(e) => fail($step, e).await,
        }
    };
}

#[embassy_executor::task]
pub async fn sensor_task(i2c: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>) -> ! {
    let mut sensor = Scd4xAsync::new(i2c, Delay);

    CHANNEL.signal(SensorState::SensorInit);
    let _ = sensor.stop_periodic_measurement().await;
    Timer::after(Duration::from_millis(500)).await;
    check!("reinit", sensor.reinit().await);
    Timer::after(Duration::from_millis(20)).await;

    check!("set altitude", sensor.set_altitude(ALTITUDE).await);
    check!(
        "set ASC",
        sensor
            .set_automatic_self_calibration(AUTOMATIC_SELF_CALIBRATION)
            .await
    );
    if let Some(temp_offset) = TEMP_OFFSET {
        check!(
            "set T offset",
            sensor.set_temperature_offset(temp_offset).await
        );
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
    // A failed self test is not fatal: the sensor often still measures fine, so
    // keep going and let the display flag the readings with a warning.
    let self_test_ok = match sensor.self_test_is_ok().await {
        Ok(true) => true,
        Ok(false) => {
            warn!("self test failed: sensor reported malfunction");
            false
        }
        Err(e) => {
            warn!("self test failed: {e}");
            false
        }
    };
    info!("self test ok: {self_test_ok}");

    CHANNEL.signal(SensorState::WarmingUp { self_test_ok });
    check!(
        "start measurement",
        sensor.start_periodic_measurement().await
    );

    loop {
        Timer::after(Duration::from_secs(5)).await;

        if sensor.data_ready_status().await.unwrap_or(false) {
            let data = check!("measurement", sensor.measurement().await);
            info!(
                "co2={}ppm temp={:.1}C humidity={:.0}%",
                data.co2, data.temperature, data.humidity
            );
            CHANNEL.signal(SensorState::Data(data));
        }
    }
}
