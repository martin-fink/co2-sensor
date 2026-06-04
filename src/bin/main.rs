#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use co2_sensor::display;
use co2_sensor::sensor;
use co2_sensor::state::{I2C_BUS, MEASUREMENT, STATE, State};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::i2c::master::{Config, I2c};
use esp_hal::main;
use esp_hal::riscv::asm::delay;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use log::{error, info};
use scd4x::Scd4x;
use ssd1306::mode::BufferedGraphicsMode;
use ssd1306::prelude::*;
use ssd1306::{I2CDisplayInterface, Ssd1306};

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);
    STATE.set_state(State::Error);
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

    //

    // let i2c = peripherals.I2C0;
    // let display_i2c = I2c::new(peripherals.I2C0, Config::default())
    //     .unwrap()
    //     .into_async()
    //     .with_sda(peripherals.GPIO4)
    //     .with_scl(peripherals.GPIO5);
    // let x = peripherals.GPIO4;

    // let interface = I2CDisplayInterface::new(i2c_display);

    // let display = Display
    //
    // let mut display: Ssd1306<_, DisplaySize128x64, BufferedGraphicsMode<_>> =
    //     Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
    //         .into_buffered_graphics_mode();
    //
    // display.init().unwrap();
    // display.clear_buffer();
    //
    // let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    //
    // embedded_graphics::text::Text::new("CO2: 657 ppm", Point::new(0, 16), style)
    //     .draw(&mut display)
    //     .unwrap();
    //
    // display.flush().unwrap();
    //
    // // TODO: Spawn some tasks
    // let _ = spawner;

    let i2c = I2c::new(peripherals.I2C0, Config::default())
        .unwrap()
        .into_async()
        .with_sda(peripherals.GPIO4)
        .with_scl(peripherals.GPIO5);

    let bus = I2C_BUS.init(Mutex::new(i2c));

    spawner.spawn(display::display_task(I2cDevice::new(bus)).unwrap());
    spawner.spawn(sensor::sensor_task(I2cDevice::new(bus)).unwrap());

    loop {
        Timer::after(Duration::from_secs(10)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}

// #![no_std]
// #![no_main]
// #![deny(
//     clippy::mem_forget,
//     reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
//     holding buffers for the duration of a data transfer."
// )]
// #![deny(clippy::large_stack_frames)]
// extern crate alloc;
//
// use esp_hal::clock::CpuClock;
// use esp_hal::delay::Delay;
// use esp_hal::gpio::{Level, Output, OutputConfig};
// use esp_hal::i2c::master::{Config, I2c};
// use esp_hal::main;
// use esp_hal::riscv::asm::delay;
// use esp_hal::time::{Duration, Instant};
// use esp_hal::timer::timg::Timer;
// use esp_println::println;
// use log::{error, info};
// use scd4x::Scd4x;
//
// #[panic_handler]
// fn panic(panic_info: &core::panic::PanicInfo) -> ! {
//     error!("{}", panic_info);
//     loop {}
// }
//
// // This creates a default app-descriptor required by the esp-idf bootloader.
// // For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
// esp_bootloader_esp_idf::esp_app_desc!();
//
// #[allow(
//     clippy::large_stack_frames,
//     reason = "it's not unusual to allocate larger buffers etc. in main"
// )]
// #[main]
// fn main() -> ! {
//     // generator version: 1.3.0
//     // generator parameters: --chip esp32c3 -o esp32c3-mini-1 -o log
//
//     esp_println::logger::init_logger_from_env();
//
//     let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
//     let peripherals = esp_hal::init(config);
//
//     // Heap allocator
//     esp_alloc::heap_allocator!(size: 72 * 1024);
//
//     // The following pins are used to bootstrap the chip. They are available
//     // for use, but check the datasheet of the module for more information on them.
//     // - GPIO2
//     // - GPIO8
//     // - GPIO9
//     // These GPIO pins are in use by some feature of the module and should not be used.
//     let _ = peripherals.GPIO11;
//     let _ = peripherals.GPIO12;
//     let _ = peripherals.GPIO13;
//     let _ = peripherals.GPIO14;
//     let _ = peripherals.GPIO15;
//     let _ = peripherals.GPIO16;
//     let _ = peripherals.GPIO17;
//
//     const SCD41_ADDR: u8 = 0x62;
//
//     let i2c = I2c::new(peripherals.I2C0, Config::default())
//         .unwrap()
//         .with_sda(peripherals.GPIO6)
//         .with_scl(peripherals.GPIO7);
//
//     let delay = Delay::new();
//     let mut scd41 = Scd4x::new(i2c, delay);
//
//
//     let _ = scd41.stop_periodic_measurement();
//     delay.delay_millis(500);
//     scd41.reinit().unwrap();
//     delay.delay_millis(20);
//
//     scd41.set_altitude(500).unwrap();
//
//     println!("serial: {:?}", scd41.serial_number());
//     println!("self test ok: {:?}", scd41.self_test_is_ok());
//     println!("asc enabled: {:?}", scd41.automatic_self_calibration());
//     println!(
//         "asc target: {:?}",
//         scd41.automatic_self_calibration_target()
//     );
//     println!("altitude: {:?}", scd41.altitude());
//     println!("temp offset: {:?}", scd41.temperature_offset());
//
//     scd41.start_periodic_measurement().unwrap();
//     println!("Waiting for first measurement...");
//
//
//     loop {
//         delay.delay_millis(10000);
//
//         match scd41.data_ready_status() {
//             Ok(true) => match scd41.measurement() {
//                 Ok(data) => {
//                     println!(
//                         "CO2: {} ppm, Temperature: {:.1} \u{00b0}C, Humidity: {:.1} %RH",
//                         data.co2, data.temperature, data.humidity
//                     );
//                 }
//                 Err(e) => {
//                     println!("SCD4x measurement error: {:?}", e);
//                 }
//             },
//             Ok(false) => {
//                 println!("Data not ready yet");
//             }
//             Err(e) => {
//                 println!("SCD4x I2C error: {:?}", e);
//             }
//         }
//     }
//
//     // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
// }
