use crate::config::{CO2_POOR, CO2_VENTILATE};

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
