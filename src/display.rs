mod boot_animation;

use core::fmt::Write as _;
use core::sync::atomic::Ordering;
use display_interface::{AsyncWriteOnlyDataCommand, DisplayError};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    mono_font::{
        MonoTextStyle,
        iso_8859_1::{FONT_6X10, FONT_9X18, FONT_10X20},
    },
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use esp_hal::Async;
use esp_hal::i2c::master::I2c;
use scd4x::types::SensorData;
use ssd1306::{I2CDisplayInterface, Ssd1306Async, mode::BufferedGraphicsModeAsync, prelude::*};

use crate::config::AirQuality;
use crate::display::boot_animation::BootAnimation;
use crate::state::{MEASUREMENT, STATE, State};

#[embassy_executor::task]
pub async fn display_task(i2c: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>) -> ! {
    let interface = I2CDisplayInterface::new(i2c);

    let mut display = Ssd1306Async::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    display.init().await.unwrap();
    display.clear_buffer();
    display.flush().await.unwrap();

    let mut boot = BootAnimation::new();

    loop {
        match STATE.get_state() {
            State::Booting => {
                boot.draw(&mut display, "booting").unwrap();
            }
            State::SensorInit => {
                boot.draw(&mut display, "starting sensor").unwrap();
            }
            State::SelfTest => {
                display.clear(BinaryColor::Off).unwrap();
                let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
                Text::with_alignment("Self Test", Point::new(64, 10), style, Alignment::Center)
                    .draw(&mut display)
                    .unwrap();

                let alt = STATE.altitude.load(Ordering::Relaxed);
                let asc = STATE.automatic_self_calibration.load(Ordering::Relaxed);
                let temp_off = STATE.get_temp_offset();

                let mut buf = heapless::String::<64>::new();
                write!(buf, "Alt: {}m", alt).unwrap();
                Text::with_alignment(buf.as_str(), Point::new(64, 26), style, Alignment::Center)
                    .draw(&mut display)
                    .unwrap();

                buf.clear();
                write!(buf, "ASC: {}", if asc { "on" } else { "off" }).unwrap();
                Text::with_alignment(buf.as_str(), Point::new(64, 40), style, Alignment::Center)
                    .draw(&mut display)
                    .unwrap();

                buf.clear();
                write!(buf, "T offset: {}C", temp_off as i32).unwrap();
                Text::with_alignment(buf.as_str(), Point::new(64, 54), style, Alignment::Center)
                    .draw(&mut display)
                    .unwrap();
            }
            State::WarmingUp => {
                boot.draw(&mut display, "warmup").unwrap();
            }
            State::Ready => {
                break;
            }
            State::Error => {}
        }
        display.flush().await.unwrap();
        Timer::after(Duration::from_millis(120)).await;
    }

    loop {
        // Blocks until the sensor task publishes a fresh reading.
        let m = MEASUREMENT.wait().await;
        draw_measurement(&mut display, &m).await.unwrap();
    }
}

/// Renders a measurement to the OLED. The CO2 reading drives a glanceable air-
/// quality cue ([`AirQuality`]): a border when ventilation is recommended, and a
/// border plus inverted colors when air quality is poor.
///
/// ```text
///        ┌──────────────────────────┐   border drawn when Ventilate/Poor
///        │                          │   (colors inverted when Poor)
///        │        823 ppm           │   big value + small "ppm" unit
///        │                          │
///        │   21.4°C        47 %     │   temperature | humidity (bigger)
///        │                          │
///        └──────────────────────────┘
/// ```
async fn draw_measurement<DI>(
    display: &mut Ssd1306Async<DI, DisplaySize128x64, BufferedGraphicsModeAsync<DisplaySize128x64>>,
    m: &SensorData,
) -> Result<(), DisplayError>
where
    DI: AsyncWriteOnlyDataCommand,
{
    const MARGIN: i32 = 6;
    // Glyph widths of the mono fonts used below.
    const BIG_W: i32 = 10; // FONT_10X20
    const UNIT_W: i32 = 6; // FONT_6X10
    const UNIT_GAP: i32 = 4;
    const BORDER_W: u32 = 2;

    let quality = AirQuality::from_co2(m.co2);

    // Inverting for "poor" means swapping the foreground/background roles: the
    // panel fills with On and everything is drawn in Off. The border uses the
    // foreground color so it stays visible in both schemes.
    let (fg, bg) = match quality {
        AirQuality::Poor => (BinaryColor::Off, BinaryColor::On),
        AirQuality::Good | AirQuality::Ventilate => (BinaryColor::On, BinaryColor::Off),
    };

    let small = MonoTextStyle::new(&FONT_6X10, fg);
    let big = MonoTextStyle::new(&FONT_10X20, fg);
    let row = MonoTextStyle::new(&FONT_9X18, fg);

    display.clear(bg)?;

    // Border as the air-quality cue: drawn for Ventilate and Poor, omitted when
    // air is Good so a clean screen reads as "nothing to do".
    if matches!(quality, AirQuality::Ventilate | AirQuality::Poor) {
        Rectangle::new(Point::zero(), Size::new(128, 64))
            .into_styled(PrimitiveStyle::with_stroke(fg, BORDER_W))
            .draw(display)?;
    }

    // CO2 hero value with a small "ppm" unit to its right. The number + unit are
    // measured and centered as a group so the layout stays balanced as the digit
    // count changes (e.g. "823" vs "1024").
    let mut buf = heapless::String::<16>::new();
    write!(buf, "{}", m.co2).unwrap();
    const UNIT: &str = "ppm";
    let num_w = buf.len() as i32 * BIG_W;
    let group_w = num_w + UNIT_GAP + UNIT.len() as i32 * UNIT_W;
    let num_x = 64 - group_w / 2;
    let baseline_y = 28;

    Text::with_text_style(
        buf.as_str(),
        Point::new(num_x, baseline_y),
        big,
        TextStyleBuilder::new()
            .baseline(Baseline::Bottom)
            .alignment(Alignment::Left)
            .build(),
    )
    .draw(display)?;

    // Unit sits on the number's baseline so it reads as a subscript-style label.
    Text::with_text_style(
        UNIT,
        Point::new(num_x + num_w + UNIT_GAP, baseline_y),
        small,
        TextStyleBuilder::new()
            .baseline(Baseline::Bottom)
            .alignment(Alignment::Left)
            .build(),
    )
    .draw(display)?;

    // Bottom row: temperature on the left, humidity on the right. Both share the
    // same top baseline so the two values line up horizontally.
    const ROW_Y: i32 = 44;
    buf.clear();
    write!(buf, "{:.1}\u{00b0}C", m.temperature).unwrap();
    Text::with_text_style(
        buf.as_str(),
        Point::new(MARGIN, ROW_Y),
        row,
        TextStyleBuilder::new()
            .baseline(Baseline::Top)
            .alignment(Alignment::Left)
            .build(),
    )
    .draw(display)?;

    buf.clear();
    write!(buf, "{:.0}%", m.humidity).unwrap();
    Text::with_text_style(
        buf.as_str(),
        Point::new(128 - MARGIN, ROW_Y),
        row,
        TextStyleBuilder::new()
            .baseline(Baseline::Top)
            .alignment(Alignment::Right)
            .build(),
    )
    .draw(display)?;

    display.flush().await?;
    Ok(())
}
