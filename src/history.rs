use crate::config::{HISTORY_BUCKET_SECS, HISTORY_POINTS, TREND_DEADBAND};
use embassy_time::Instant;
use heapless::HistoryBuffer;

/// Direction of the CO2 trend relative to the recent history baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trend {
    Up,
    Flat,
    Down,
}

/// Downsampled CO2 history feeding the average, trend arrow, and sparkline.
///
/// Raw readings arrive every few seconds; they are accumulated into time buckets
/// of [`config::HISTORY_BUCKET_SECS`] and only the per-bucket mean is retained, so
/// [`config::HISTORY_POINTS`] points cover the full window with little RAM.
pub struct History {
    points: HistoryBuffer<u16, { HISTORY_POINTS }>,
    bucket_sum: u32,
    bucket_count: u32,
    bucket_start: Instant,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self {
            points: HistoryBuffer::new(),
            bucket_sum: 0,
            bucket_count: 0,
            bucket_start: Instant::now(),
        }
    }

    /// Adds a reading to the current bucket, closing the bucket into a stored
    /// point once `HISTORY_BUCKET_SECS` have elapsed.
    pub fn push(&mut self, co2: u16, now: Instant) {
        self.bucket_sum += co2 as u32;
        self.bucket_count += 1;

        let elapsed = now.duration_since(self.bucket_start).as_secs();
        if elapsed >= HISTORY_BUCKET_SECS {
            let mean = (self.bucket_sum / self.bucket_count) as u16;
            self.points.write(mean);
            self.bucket_sum = 0;
            self.bucket_count = 0;
            self.bucket_start = now;
        }
    }

    /// Number of retained history points.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether any history points have been retained yet.
    pub fn is_empty(&self) -> bool {
        self.points.len() == 0
    }

    /// Retained points iterated oldest first, for rendering the graph.
    pub fn points_oldest_first(&self) -> impl Iterator<Item = u16> + '_ {
        self.points.oldest_ordered().copied()
    }

    /// Mean of all retained points, or `None` until the first bucket closes.
    pub fn average(&self) -> Option<u16> {
        let len = self.points.len();
        if len == 0 {
            return None;
        }
        let sum: u32 = self.points.oldest_ordered().map(|&p| p as u32).sum();
        Some((sum / len as u32) as u16)
    }

    /// Trend of `latest` against the oldest retained point, with a deadband so
    /// small fluctuations read as steady.
    pub fn trend(&self, latest: u16) -> Trend {
        let Some(&baseline) = self.points.oldest_ordered().next() else {
            return Trend::Flat;
        };
        let deadband = TREND_DEADBAND as i32;
        let delta = latest as i32 - baseline as i32;
        if delta > deadband {
            Trend::Up
        } else if delta < -deadband {
            Trend::Down
        } else {
            Trend::Flat
        }
    }
}
