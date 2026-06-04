mod boot_animation;

use crate::config;
use crate::config::AirQuality;
use crate::display::boot_animation::BootAnimation;
use crate::history::History;
use crate::state::{MEASUREMENT, STATE, State};
use core::fmt::Write as _;
use core::sync::atomic::Ordering;
use display_interface::{AsyncWriteOnlyDataCommand, DisplayError};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::{
    mono_font::{
        MonoTextStyle,
        iso_8859_1::{FONT_4X6, FONT_6X10, FONT_9X18, FONT_10X20},
    },
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use esp_hal::Async;
use esp_hal::i2c::master::I2c;
use scd4x::types::SensorData;
use ssd1306::{I2CDisplayInterface, Ssd1306Async, mode::BufferedGraphicsModeAsync, prelude::*};

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

    let mut history = History::new();

    loop {
        // Blocks until the sensor task publishes a fresh reading.
        let m = MEASUREMENT.wait().await;
        history.push(m.co2, Instant::now());
        draw_measurement(&mut display, &m, &history).await.unwrap();
    }
}

/// Renders a measurement to the OLED: current CO2 value on the left, an outlined
/// history graph on the right. The reading also drives a glanceable air-quality
/// cue ([`AirQuality`]): a border when ventilation is recommended, and a border
/// plus inverted colors when air quality is poor.
async fn draw_measurement<DI>(
    display: &mut Ssd1306Async<DI, DisplaySize128x64, BufferedGraphicsModeAsync<DisplaySize128x64>>,
    m: &SensorData,
    hist: &History,
) -> Result<(), DisplayError>
where
    DI: AsyncWriteOnlyDataCommand,
{
    const MARGIN: i32 = 6;
    const BIG_W: i32 = 10; // FONT_10X20 glyph width
    const BORDER_W: u32 = 2;

    let quality = AirQuality::from_co2(m.co2);

    // Inverting for "poor" means swapping the foreground/background roles: the
    // panel fills with On and everything is drawn in Off. The border uses the
    // foreground color so it stays visible in both schemes.
    let (fg, bg) = match quality {
        AirQuality::Poor => (BinaryColor::Off, BinaryColor::On),
        AirQuality::Good | AirQuality::Ventilate => (BinaryColor::On, BinaryColor::Off),
    };

    let tiny = MonoTextStyle::new(&FONT_4X6, fg);
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

    // --- Upper left: "CO2" label above the current value ---

    // Label "CO" (small) with a subscript "2" (tiny, dropped to the baseline).
    let label_x = MARGIN;
    let label_y = MARGIN * 2;
    Text::with_text_style(
        "CO",
        Point::new(label_x + 2, label_y),
        small,
        TextStyleBuilder::new()
            .baseline(Baseline::Bottom)
            .alignment(Alignment::Left)
            .build(),
    )
    .draw(display)?;
    Text::with_text_style(
        "2",
        Point::new(label_x + 2 * 7, label_y),
        tiny,
        TextStyleBuilder::new()
            .baseline(Baseline::Bottom)
            .alignment(Alignment::Left)
            .build(),
    )
    .draw(display)?;

    // Current CO2 value (big).
    let mut buf = heapless::String::<16>::new();
    write!(buf, "{}", m.co2).unwrap();

    const UNIT_GAP: i32 = 8;
    const UNIT: &str = "ppm";

    let num_w = buf.len() as i32 * BIG_W;
    // let num_x = 64 - group_w / 2;
    let baseline_y = 28;

    Text::with_text_style(
        buf.as_str(),
        Point::new(MARGIN, 31),
        big,
        TextStyleBuilder::new()
            .baseline(Baseline::Bottom)
            .alignment(Alignment::Left)
            .build(),
    )
    .draw(display)?;
    // `ppm` sits on the number's baseline so it reads as a subscript-style label.
    Text::with_text_style(
        UNIT,
        Point::new(num_w + UNIT_GAP, baseline_y),
        small,
        TextStyleBuilder::new()
            .baseline(Baseline::Bottom)
            .alignment(Alignment::Left)
            .build(),
    )
    .draw(display)?;

    // --- Upper right: outlined history graph ---
    // Sized to fill the upper-right, clear of the value.
    let gx = MARGIN + (4 + 2) * BIG_W + 4;
    let graph = Rectangle::new(
        Point::new(gx, MARGIN),
        Size::new((128 - MARGIN - gx) as u32, (31 - MARGIN) as u32),
    );
    draw_graph(display, hist, graph, fg)?;

    // --- Bottom row: temperature on the left, humidity on the right ---

    Text::with_text_style(
        "Temp",
        Point::new(label_x + 2, ROW_Y),
        small,
        TextStyleBuilder::new()
            .baseline(Baseline::Bottom)
            .alignment(Alignment::Left)
            .build(),
    )
    .draw(display)?;

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

    Text::with_text_style(
        "Humidity",
        Point::new(128 - MARGIN, ROW_Y),
        small,
        TextStyleBuilder::new()
            .baseline(Baseline::Bottom)
            .alignment(Alignment::Right)
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

/// Draws the history as an outlined sparkline filling `rect`: a 1px border with
/// one vertical bar per retained point inside, scaled over a fixed ppm range.
fn draw_graph<D>(
    display: &mut D,
    hist: &History,
    rect: Rectangle,
    fg: BinaryColor,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    const PPM_MIN: i32 = 400;
    const PPM_MAX: i32 = 2000;

    rect.into_styled(PrimitiveStyle::with_stroke(fg, 1))
        .draw(display)?;

    let len = hist.len() as i32;
    if len == 0 {
        return Ok(());
    }

    let full_len = config::HISTORY_POINTS as i32;
    if full_len <= 0 {
        return Ok(());
    }

    let ix = rect.top_left.x + 1;
    let iy = rect.top_left.y + 1;
    let iw = rect.size.width as i32 - 2;
    let ih = rect.size.height as i32 - 2;

    if iw <= 0 || ih <= 0 {
        return Ok(());
    }

    let stroke = PrimitiveStyle::with_stroke(fg, 1);

    // Shift current samples to the right.
    // Example: full_len = 60, len = 10 => first sample starts at x index 50.
    let start = full_len - len;

    let mut prev: Option<Point> = None;

    for (i, p) in hist.points_oldest_first().enumerate() {
        let x_index = start + i as i32;

        let x = if full_len == 1 {
            ix
        } else {
            ix + (x_index * (iw - 1)) / (full_len - 1)
        };

        let clamped = (p as i32).clamp(PPM_MIN, PPM_MAX);

        let y_offset = ((clamped - PPM_MIN) * (ih - 1)) / (PPM_MAX - PPM_MIN);
        let y = iy + ih - 1 - y_offset;

        let current = Point::new(x, y);

        if let Some(prev) = prev {
            Line::new(prev, current).into_styled(stroke).draw(display)?;
        } else {
            Pixel(current, fg).draw(display)?;
        }

        prev = Some(current);
    }

    Ok(())
}
