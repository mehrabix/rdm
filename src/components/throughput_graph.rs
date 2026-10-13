//! Aggregate throughput sparkline for the status bar.

use std::time::Duration;

use dioxus::prelude::*;
use rdm_core::format::human_speed;

use crate::state::Downloads;

const SAMPLES: usize = 120;

#[component]
pub fn ThroughputGraph(state: Downloads) -> Element {
    let history = use_signal(Vec::<f64>::new);

    // Self-contained sampler: once every 500 ms, record the aggregate speed.
    use_future(move || async move {
        let mut history = history;
        loop {
            let speed = state.total_speed();
            history.with_mut(|samples| {
                samples.push(speed);
                if samples.len() > SAMPLES {
                    samples.remove(0);
                }
            });
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });

    let samples = history.read().clone();
    let peak = samples.iter().cloned().fold(1.0_f64, f64::max);
    let count = samples.len();
    let points = polyline(&samples, peak, count);
    let label = human_speed(samples.last().copied().unwrap_or(0.0));

    rsx! {
        div { class: "flex items-center gap-2",
            svg {
                class: "h-7 w-24 text-primary",
                view_box: "0 0 100 30",
                polyline { points, fill: "none", stroke: "currentColor", stroke_width: "1.5" }
            }
            span { class: "w-16 text-end text-xs tabular-nums text-muted-foreground", "{label}" }
        }
    }
}

fn polyline(samples: &[f64], peak: f64, count: usize) -> String {
    if count < 2 {
        return String::new();
    }
    let span = (count - 1) as f64;
    samples
        .iter()
        .enumerate()
        .map(|(i, value)| {
            let x = i as f64 / span * 100.0;
            let y = 28.0 - (value / peak) * 26.0;
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}
