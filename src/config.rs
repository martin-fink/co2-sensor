pub const ALTITUDE: u16 = 500;
pub const AUTOMATIC_SELF_CALIBRATION: Option<bool> = None;
pub const TEMP_OFFSET: Option<f32> = None;

/// CO2 level (ppm) at and above which ventilation is recommended. The display
/// draws a border around the screen as a glanceable cue to open the window.
pub const CO2_VENTILATE: u16 = 1000;

/// CO2 level (ppm) at and above which air quality is considered poor. The
/// display draws the border *and* inverts the colors for a stronger alert.
pub const CO2_POOR: u16 = 1500;

/// Number of points retained for the history graph / average / trend.
pub const HISTORY_POINTS: usize = 60;

/// Seconds of readings averaged into each history point.
/// `HISTORY_POINTS * HISTORY_BUCKET_SECS` is the total window.
pub const HISTORY_BUCKET_SECS: u64 = 30;

/// ppm change (vs. the trend baseline) below which the trend reads as steady.
pub const TREND_DEADBAND: u16 = 30;
