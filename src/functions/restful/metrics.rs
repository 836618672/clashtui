//! Bounded samples of core counters. A decrease starts a new counter epoch.
use super::connection::ConnInfo;
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Default)]
pub struct Metrics {
    previous: Option<(Instant, u64, u64)>,
    baseline: Option<(u64, u64)>,
    pub download: u64,
    pub upload: u64,
    pub download_speed: u64,
    pub upload_speed: u64,
    pub connections: usize,
    pub history: VecDeque<u64>,
}

impl Metrics {
    pub fn sample(&mut self, info: &ConnInfo, now: Instant) {
        let totals = (info.download_total, info.upload_total);
        let reset = self
            .previous
            .is_some_and(|(_, down, up)| totals.0 < down || totals.1 < up);
        if self.baseline.is_none() || reset {
            self.baseline = Some(totals);
            self.history.clear();
        }
        self.download_speed = 0;
        self.upload_speed = 0;
        if let Some((time, down, up)) = self.previous.filter(|_| !reset) {
            let elapsed = now.saturating_duration_since(time).as_secs_f64();
            if elapsed > 0.0 {
                self.download_speed = ((totals.0 - down) as f64 / elapsed) as u64;
                self.upload_speed = ((totals.1 - up) as f64 / elapsed) as u64;
            }
        }
        let baseline = self.baseline.unwrap_or(totals);
        self.download = totals.0.saturating_sub(baseline.0);
        self.upload = totals.1.saturating_sub(baseline.1);
        self.connections = info.connections.as_ref().map_or(0, Vec::len);
        self.previous = Some((now, totals.0, totals.1));
        self.history
            .push_back(self.download_speed.saturating_add(self.upload_speed));
        if self.history.len() > 120 {
            self.history.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn uses_elapsed_time_and_resets_after_core_counter_restart() {
        let mut metrics = Metrics::default();
        let now = Instant::now();
        let mut info = ConnInfo {
            download_total: 100,
            upload_total: 50,
            connections: None,
        };
        metrics.sample(&info, now);
        info.download_total = 300;
        info.upload_total = 90;
        metrics.sample(&info, now + Duration::from_secs(2));
        assert_eq!((metrics.download_speed, metrics.upload_speed), (100, 20));
        assert_eq!((metrics.download, metrics.upload), (200, 40));
        info.download_total = 5;
        metrics.sample(&info, now + Duration::from_secs(3));
        assert_eq!((metrics.download_speed, metrics.download), (0, 0));
        assert_eq!(metrics.history.len(), 1);
        for i in 4..200 {
            metrics.sample(&info, now + Duration::from_secs(i));
        }
        assert_eq!(metrics.history.len(), 120);
    }
}
