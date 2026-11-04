mod boot_animation;

use crate::air_quality::AirQuality;
use crate::config;
use crate::display::boot_animation::BootAnimation;
use crate::history::{History, Trend};
use crate::state::{CHANNEL, SensorState};
use core::fmt::Write as _;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::{Duration, Instant, with_timeout};
use embedded_graphics::{
    mono_font::{
        MonoTextStyle,
        iso_8859_1::{FONT_4X6, FONT_6X10, FONT_9X18, FONT_10X20},
    },
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle, Triangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use esp_hal::Async;
use esp_hal::i2c::master::I2c;
use scd4x::types::SensorData;
use ssd1306::{I2CDisplayInterface, Ssd1306Async, prelude::*};

/// Redraw interval for animated screens (spinner, self-test progress). A full
/// frame is ~1 KiB over the shared I2C bus, so keep this modest.
const FRAME: Duration = Duration::from_millis(250);

/// Duration of the SCD4x self test (datasheet: 10 s), used for the progress bar.
const SELF_TEST: Duration = Duration::from_secs(10);

#[embassy_executor::task]
pub async fn display_task(i2c: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>) -> ! {
    let interface = I2CDisplayInterface::new(i2c);

    let mut display = Ssd1306Async::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    display.init().await.unwrap();
    display.clear_buffer();
    display.flush().await.unwrap();

    let mut boot = BootAnimation::new();
    let mut history = History::new();
    let mut self_test_ok = true;

    let mut state = CHANNEL.wait().await;
    let mut entered = Instant::now();
    loop {
        if let SensorState::WarmingUp { self_test_ok: ok } = state {
            self_test_ok = ok;
        }

        match &state {
            SensorState::Booting => {
                boot.draw(&mut display, "booting").unwrap();
            }
            SensorState::SensorInit => {
                boot.draw(&mut display, "starting sensor").unwrap();
            }
            SensorState::SelfTest {
                temp_offset,
                altitude,
                automatic_self_calibration,
            } => {
                draw_self_test(
                    &mut display,
                    *temp_offset,
                    *altitude,
                    *automatic_self_calibration,
                    entered.elapsed(),
                )
                .unwrap();
            }
            SensorState::WarmingUp { .. } => {
                boot.draw(&mut display, "warmup").unwrap();
                if !self_test_ok {
                    draw_warning(&mut display, Point::new(14, 30)).unwrap();
                    draw_warning(&mut display, Point::new(114, 30)).unwrap();
                }
            }
            SensorState::Data(data) => {
                draw_measurement(&mut display, data, &history, self_test_ok).unwrap();
            }
            SensorState::Error { step, detail } => {
                draw_error(&mut display, step, detail).unwrap();
            }
        }
        display.flush().await.unwrap();

        // Animated screens redraw every frame until the next state arrives;
        // static ones just wait for it.
        let animated = matches!(
            state,
            SensorState::Booting
                | SensorState::SensorInit
                | SensorState::SelfTest { .. }
                | SensorState::WarmingUp { .. }
        );
        let next = if animated {
            with_timeout(FRAME, CHANNEL.wait()).await.ok()
        } else {
            Some(CHANNEL.wait().await)
        };
        if let Some(next) = next {
            if let SensorState::Data(data) = &next {
                history.push(data.co2, Instant::now());
            }
            state = next;
            entered = Instant::now();
        }
    }
}

/// Shows the sensor configuration and a progress bar while the self test runs.
fn draw_self_test<D>(
    display: &mut D,
    temp_offset: f32,
    altitude: u16,
    automatic_self_calibration: bool,
    elapsed: Duration,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    display.clear(BinaryColor::Off)?;
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::with_alignment("Self Test", Point::new(64, 10), style, Alignment::Center)
        .draw(display)?;

    let mut buf = heapless::String::<64>::new();
    write!(buf, "Alt: {}m", altitude).unwrap();
    Text::with_alignment(buf.as_str(), Point::new(64, 23), style, Alignment::Center)
        .draw(display)?;

    buf.clear();
    write!(
        buf,
        "ASC: {}",
        if automatic_self_calibration {
            "on"
        } else {
            "off"
        }
    )
    .unwrap();
    Text::with_alignment(buf.as_str(), Point::new(64, 35), style, Alignment::Center)
        .draw(display)?;

    buf.clear();
    write!(buf, "T offset: {:.1}C", temp_offset).unwrap();
    Text::with_alignment(buf.as_str(), Point::new(64, 47), style, Alignment::Center)
        .draw(display)?;

    // Progress bar: outline plus a fill proportional to the elapsed test time.
    const BAR_W: u32 = 100;
    let bar = Rectangle::new(Point::new(14, 55), Size::new(BAR_W, 7));
    bar.into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(display)?;
    let progress = elapsed.as_millis().min(SELF_TEST.as_millis()) as u32;
    let fill = (BAR_W - 4) * progress / SELF_TEST.as_millis() as u32;
    if fill > 0 {
        Rectangle::new(bar.top_left + Point::new(2, 2), Size::new(fill, 3))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(display)?;
    }
    Ok(())
}

/// Draws a filled warning triangle with a "!" cut out, centered on `center`
/// (13x11 px). Used to flag readings from a sensor that failed its self test.
fn draw_warning<D>(display: &mut D, center: Point) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    Triangle::new(
        center + Point::new(0, -5),
        center + Point::new(-6, 5),
        center + Point::new(6, 5),
    )
    .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
    .draw(display)?;
    Line::new(center + Point::new(0, -2), center + Point::new(0, 1))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 1))
        .draw(display)?;
    Pixel(center + Point::new(0, 3), BinaryColor::Off).draw(display)
}

/// Draws a 15x15 window icon (frame with a cross) centered on `center`, as the
/// "open the window" cue. `filled` draws solid panes for a stronger alert.
fn draw_window<D>(
    display: &mut D,
    center: Point,
    filled: bool,
    fg: BinaryColor,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let frame = Rectangle::with_center(center, Size::new(15, 15));
    let cross = if filled {
        frame
            .into_styled(PrimitiveStyle::with_fill(fg))
            .draw(display)?;
        // Cut the cross out of the filled frame, leaving four solid panes.
        PrimitiveStyle::with_stroke(fg.invert(), 1)
    } else {
        frame
            .into_styled(PrimitiveStyle::with_stroke(fg, 1))
            .draw(display)?;
        PrimitiveStyle::with_stroke(fg, 1)
    };
    Line::new(center + Point::new(0, -6), center + Point::new(0, 6))
        .into_styled(cross)
        .draw(display)?;
    Line::new(center + Point::new(-6, 0), center + Point::new(6, 0))
        .into_styled(cross)
        .draw(display)?;
    Ok(())
}

/// Draws the trend as a small arrow centered on `center`: a triangle pointing
/// up or down, or a short dash when steady.
fn draw_trend<D>(
    display: &mut D,
    trend: Trend,
    center: Point,
    fg: BinaryColor,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let fill = PrimitiveStyle::with_fill(fg);
    match trend {
        Trend::Up => Triangle::new(
            center + Point::new(0, -3),
            center + Point::new(-4, 2),
            center + Point::new(4, 2),
        )
        .into_styled(fill)
        .draw(display),
        Trend::Down => Triangle::new(
            center + Point::new(0, 3),
            center + Point::new(-4, -2),
            center + Point::new(4, -2),
        )
        .into_styled(fill)
        .draw(display),
        Trend::Flat => Line::new(center + Point::new(-3, 0), center + Point::new(3, 0))
            .into_styled(PrimitiveStyle::with_stroke(fg, 1))
            .draw(display),
    }
}

/// Shows a failed sensor step and its error detail, wrapped to fit the screen.
fn draw_error<D>(display: &mut D, step: &str, detail: &str) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    const LINE_CHARS: usize = 128 / 4; // FONT_4X6 glyph width

    display.clear(BinaryColor::Off)?;
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::with_alignment("Sensor Error", Point::new(64, 10), style, Alignment::Center)
        .draw(display)?;
    Text::with_alignment(step, Point::new(64, 24), style, Alignment::Center).draw(display)?;

    let tiny = MonoTextStyle::new(&FONT_4X6, BinaryColor::On);
    for (i, line) in detail.as_bytes().chunks(LINE_CHARS).take(4).enumerate() {
        let line = core::str::from_utf8(line).unwrap_or("?");
        Text::new(line, Point::new(0, 36 + i as i32 * 7), tiny).draw(display)?;
    }
    Ok(())
}

/// Renders a measurement to the OLED: current CO2 value on the left, an outlined
/// history graph on the right. The reading also drives a glanceable air-quality
/// cue ([`AirQuality`]): a window icon between temperature and humidity, outlined
/// when ventilation is recommended and filled when air quality is poor. A warning
/// triangle next to the CO2 label flags a failed self test.
fn draw_measurement<D>(
    display: &mut D,
    m: &SensorData,
    hist: &History,
    self_test_ok: bool,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    const MARGIN: i32 = 6;
    const BIG_W: i32 = 10; // FONT_10X20 glyph width

    let quality = AirQuality::from_co2(m.co2);

    let (fg, bg) = (BinaryColor::On, BinaryColor::Off);

    let tiny = MonoTextStyle::new(&FONT_4X6, fg);
    let small = MonoTextStyle::new(&FONT_6X10, fg);
    let big = MonoTextStyle::new(&FONT_10X20, fg);
    let row = MonoTextStyle::new(&FONT_9X18, fg);

    display.clear(bg)?;

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
    let unit_x = num_w + UNIT_GAP;
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
        Point::new(unit_x, baseline_y),
        small,
        TextStyleBuilder::new()
            .baseline(Baseline::Bottom)
            .alignment(Alignment::Left)
            .build(),
    )
    .draw(display)?;

    // Trend arrow centered above `ppm` (3 glyphs of FONT_6X10).
    draw_trend(display, hist.trend(m.co2), Point::new(unit_x + 9, 12), fg)?;

    // --- Upper right: outlined history graph ---
    // Sized to fill the upper-right, clear of the value.
    let gx = MARGIN + (4 + 2) * BIG_W + 4;
    let graph = Rectangle::new(
        Point::new(gx, MARGIN),
        Size::new((128 - MARGIN - gx) as u32, (31 - MARGIN) as u32),
    );
    draw_graph(display, hist, graph, fg)?;

    // --- Bottom row: temperature on the left, humidity on the right ---

    const ROW_Y: i32 = 44;

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

    // Air-quality cue between the temperature and humidity values; omitted when
    // air is Good so a clean screen reads as "nothing to do".
    if quality != AirQuality::Good {
        draw_window(display, Point::new(77, 52), quality == AirQuality::Poor, fg)?;
    }

    // Right of the CO2 label: the readings themselves may be unreliable.
    if !self_test_ok {
        draw_warning(display, Point::new(32, 7))?;
    }

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
    let y_for = |ppm: i32| {
        let clamped = ppm.clamp(PPM_MIN, PPM_MAX);
        iy + ih - 1 - ((clamped - PPM_MIN) * (ih - 1)) / (PPM_MAX - PPM_MIN)
    };

    // Dotted reference line at the ventilation threshold.
    let threshold_y = y_for(config::CO2_VENTILATE as i32);
    for x in (ix..ix + iw).step_by(3) {
        Pixel(Point::new(x, threshold_y), fg).draw(display)?;
    }

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

        let current = Point::new(x, y_for(p as i32));

        if let Some(prev) = prev {
            Line::new(prev, current).into_styled(stroke).draw(display)?;
        } else {
            Pixel(current, fg).draw(display)?;
        }

        prev = Some(current);
    }

    Ok(())
}
