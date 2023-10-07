use std::time::{Duration, Instant};

const MAX_RTT: Duration = Duration::from_millis(100);
const MIN_RTT: Duration = Duration::from_millis(10);
const INFLATE_RTT_PERCENTAGE: u32 = 10; //10%

pub struct RttTracker {
    total_rtt: Duration,
    num_measurements: u32,
}

impl RttTracker {
    pub fn new() -> Self {
        RttTracker {
            total_rtt: (MIN_RTT + MAX_RTT) / 2,
            num_measurements: 1,
        }
    }

    pub fn record_rtt(&mut self, sent_at: Instant, received_at: Instant) {
        let rtt = received_at.duration_since(sent_at);
        self.total_rtt += rtt;
        self.num_measurements += 1;
    }

    pub fn average_rtt(&self) -> Duration {
        self.total_rtt / self.num_measurements
    }

    pub fn recommended_max_rtt(&self) -> Duration {
        let average_rtt = self.total_rtt / self.num_measurements;

        Duration::clamp(
            average_rtt + (average_rtt / INFLATE_RTT_PERCENTAGE),
            MIN_RTT,
            MAX_RTT,
        )
    }
}
