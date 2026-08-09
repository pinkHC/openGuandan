use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub(crate) struct RateWindow {
    started_at: Instant,
    last_seen: Instant,
    count: u32,
}

impl RateWindow {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            started_at: now,
            last_seen: now,
            count: 0,
        }
    }

    pub(crate) fn consume(&mut self, now: Instant, duration: Duration, maximum: u32) -> bool {
        if self.window_elapsed(now, duration) {
            self.started_at = now;
            self.count = 0;
        }
        self.last_seen = now;
        self.count = self.count.saturating_add(1);
        self.count > maximum
    }

    pub(crate) fn touch(&mut self, now: Instant) {
        self.last_seen = now;
    }

    pub(crate) fn is_idle(&self, now: Instant, retention: Duration) -> bool {
        now.duration_since(self.last_seen) >= retention
    }

    pub(crate) fn window_elapsed(&self, now: Instant, duration: Duration) -> bool {
        now.duration_since(self.started_at) >= duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_window_limits_then_resets_at_the_boundary() {
        let now = Instant::now();
        let duration = Duration::from_secs(10);
        let mut window = RateWindow::new(now);

        assert!(!window.consume(now, duration, 2));
        assert!(!window.consume(now, duration, 2));
        assert!(window.consume(now, duration, 2));
        assert!(!window.consume(now + duration, duration, 2));
        assert!(window.is_idle(now + duration * 2, duration));
    }
}
