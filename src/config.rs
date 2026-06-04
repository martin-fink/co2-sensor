pub const ALTITUDE: u16 = 500;
pub const AUTOMATIC_SELF_CALIBRATION: Option<bool> = None;
pub const TEMP_OFFSET: Option<f32> = None;

/// CO2 level (ppm) at and above which ventilation is recommended. The display
/// draws a border around the screen as a glanceable cue to open the window.
pub const CO2_VENTILATE: u16 = 1000;

/// CO2 level (ppm) at and above which air quality is considered poor. The
/// display draws the border *and* inverts the colors for a stronger alert.
pub const CO2_POOR: u16 = 1500;

/// Air quality bucket derived from a CO2 reading, used to drive the display cue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AirQuality {
    /// Below [`CO2_VENTILATE`]: no special cue.
    Good,
    /// At or above [`CO2_VENTILATE`]: border shown, open a window.
    Ventilate,
    /// At or above [`CO2_POOR`]: border shown and colors inverted.
    Poor,
}

impl AirQuality {
    /// Classifies a CO2 reading (ppm) against the configured thresholds.
    pub const fn from_co2(co2: u16) -> Self {
        if co2 >= CO2_POOR {
            Self::Poor
        } else if co2 >= CO2_VENTILATE {
            Self::Ventilate
        } else {
            Self::Good
        }
    }
}
