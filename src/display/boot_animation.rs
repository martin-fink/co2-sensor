use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle},
    text::{Alignment, Text},
};

pub struct BootAnimation {
    frame: usize,
}

impl BootAnimation {
    pub fn new() -> Self {
        Self { frame: 0 }
    }

    pub fn draw<D>(&mut self, display: &mut D, status: &str) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        display.clear(BinaryColor::Off)?;

        let text = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

        Text::with_alignment("CO2 Sensor", Point::new(64, 18), text, Alignment::Center)
            .draw(display)?;

        Text::with_alignment(status, Point::new(64, 54), text, Alignment::Center).draw(display)?;

        self.draw_spinner(display, Point::new(64, 36))?;

        self.frame = self.frame.wrapping_add(1);
        Ok(())
    }

    fn draw_spinner<D>(&self, display: &mut D, center: Point) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        let style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);

        Circle::new(center - Size::new(8, 8), 16)
            .into_styled(style)
            .draw(display)?;

        let dirs = [
            Point::new(0, -6),
            Point::new(4, -4),
            Point::new(6, 0),
            Point::new(4, 4),
            Point::new(0, 6),
            Point::new(-4, 4),
            Point::new(-6, 0),
            Point::new(-4, -4),
        ];

        let d = dirs[self.frame % dirs.len()];

        Line::new(center, center + d)
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 2))
            .draw(display)?;

        Ok(())
    }
}
