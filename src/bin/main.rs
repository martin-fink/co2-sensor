#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use co2_sensor::display;
use co2_sensor::state::{CHANNEL, I2C_BUS, SensorState};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::i2c::master::{Config, I2c};
use esp_hal::timer::timg::TimerGroup;
use log::{error, info};

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);
    CHANNEL.signal(SensorState::Error);
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32c3 -o esp32c3-mini-1 -o unstable-hal -o alloc -o embassy -o log

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO2
    // - GPIO8
    // - GPIO9
    // These GPIO pins are in use by some feature of the module and should not be used.
    let _ = peripherals.GPIO11;
    let _ = peripherals.GPIO12;
    let _ = peripherals.GPIO13;
    let _ = peripherals.GPIO14;
    let _ = peripherals.GPIO15;
    let _ = peripherals.GPIO16;
    let _ = peripherals.GPIO17;

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 66320);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialized!");

    let i2c = I2c::new(peripherals.I2C0, Config::default())
        .unwrap()
        .into_async()
        .with_sda(peripherals.GPIO4)
        .with_scl(peripherals.GPIO5);

    let bus = I2C_BUS.init(Mutex::new(i2c));

    CHANNEL.signal(SensorState::Booting);

    spawner.spawn(display::display_task(I2cDevice::new(bus)).unwrap());
    #[cfg(not(feature = "debug-test-data"))]
    spawner.spawn(co2_sensor::sensor::sensor_task(I2cDevice::new(bus)).unwrap());
    #[cfg(feature = "debug-test-data")]
    spawner.spawn(debug_test_values().unwrap());

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

#[cfg(feature = "debug-test-data")]
#[embassy_executor::task]
async fn debug_test_values() -> ! {
    use co2_sensor::state::CHANNEL;
    use scd4x::types::SensorData;

    fn xorshift32(state: &mut i16) -> i16 {
        let mut x = *state;
        x ^= x.overflowing_shl(13).0;
        x ^= x.overflowing_shr(17).0;
        x ^= x.overflowing_shl(5).0;
        *state = x;
        x
    }

    CHANNEL.signal(SensorState::Data(SensorData {
        co2: 900,
        temperature: 23.0,
        humidity: 39.0,
    }));
    Timer::after(Duration::from_millis(500)).await;
    CHANNEL.signal(SensorState::Data(SensorData {
        co2: 1000,
        temperature: 23.0,
        humidity: 39.0,
    }));

    let mut co2 = 1000;
    let mut change = 10i16;
    loop {
        CHANNEL.signal(SensorState::Data(SensorData {
            co2,
            temperature: 23.0,
            humidity: 39.0,
        }));
        Timer::after(Duration::from_millis(500)).await;
        co2 = (co2 as i16 + xorshift32(&mut change) % 16) as u16;
    }
}
