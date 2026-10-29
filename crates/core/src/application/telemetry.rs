//! Observabilidad y métricas internas del core (ADR 0004 §3).
//!
//! Usa `AtomicU64` para contadores y `hdrhistogram` para percentiles de latencia.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use hdrhistogram::Histogram;
use kiti_protocol::CoreMetrics;

pub struct Telemetry {
    pub events_total: AtomicU64,
    pub hook_rejected_total: AtomicU64,
    pub ipc_flush_total: AtomicU64,
    pub metrics_msgs_total: AtomicU64,
    pub mails_total: AtomicU64,
    hook_to_publish_hist: Mutex<Histogram<u64>>,
    last_sample_time: Mutex<Instant>,
    last_sample_events: AtomicU64,
}

impl Telemetry {
    pub fn new() -> Self {
        // Histograma de 1 ms a 60_000 ms (1 min) con 3 dígitos significativos
        let hist = Histogram::<u64>::new_with_bounds(1, 60_000, 3)
            .expect("bounds válidos para histograma");
        Self {
            events_total: AtomicU64::new(0),
            hook_rejected_total: AtomicU64::new(0),
            ipc_flush_total: AtomicU64::new(0),
            metrics_msgs_total: AtomicU64::new(0),
            mails_total: AtomicU64::new(0),
            hook_to_publish_hist: Mutex::new(hist),
            last_sample_time: Mutex::new(Instant::now()),
            last_sample_events: AtomicU64::new(0),
        }
    }

    pub fn record_event(&self) {
        self.events_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Un `kiti_send` aceptado (ADR 0017).
    pub fn record_mail(&self) {
        self.mails_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_publish_latency_ms(&self, ms: u64) {
        let val = ms.clamp(1, 60_000);
        if let Ok(mut hist) = self.hook_to_publish_hist.lock() {
            let _ = hist.record(val);
        }
    }

    pub fn record_flush(&self) {
        self.ipc_flush_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot_core_metrics(
        &self,
        agents_count: u32,
        rss_bytes: u64,
        cpu_percent: f64,
        mail_pending: u32,
    ) -> CoreMetrics {
        self.metrics_msgs_total.fetch_add(1, Ordering::Relaxed);
        let now = Instant::now();
        let current_events = self.events_total.load(Ordering::Relaxed);
        let events_per_sec = {
            let mut last_time = self.last_sample_time.lock().unwrap();
            let elapsed = now.duration_since(*last_time).as_secs_f64();
            let prev_events = self
                .last_sample_events
                .swap(current_events, Ordering::Relaxed);
            *last_time = now;
            if elapsed > 0.001 {
                current_events.saturating_sub(prev_events) as f64 / elapsed
            } else {
                0.0
            }
        };

        let hook_to_publish_ms_p95 = {
            let hist = self.hook_to_publish_hist.lock().unwrap();
            hist.value_at_quantile(0.95) as f64
        };

        CoreMetrics {
            agents: agents_count,
            events_per_sec,
            hook_to_publish_ms_p95,
            rss_bytes,
            cpu_percent,
            mails_total: self.mails_total.load(Ordering::Relaxed),
            mail_pending,
        }
    }
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_registra_eventos_y_percentiles() {
        let telem = Telemetry::new();
        telem.record_event();
        telem.record_event();
        assert_eq!(telem.events_total.load(Ordering::Relaxed), 2);

        telem.record_publish_latency_ms(10);
        telem.record_publish_latency_ms(20);
        telem.record_publish_latency_ms(100);

        let metrics = telem.snapshot_core_metrics(5, 1024 * 1024, 0.5, 0);
        assert_eq!(metrics.agents, 5);
        assert_eq!(metrics.rss_bytes, 1024 * 1024);
        assert_eq!(metrics.cpu_percent, 0.5);
        assert!(metrics.hook_to_publish_ms_p95 >= 20.0);
        assert_eq!(telem.metrics_msgs_total.load(Ordering::Relaxed), 1);
    }
}
