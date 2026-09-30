//! What it costs to rerank: the time `OnnxReranker::score` takes for a number of passages, per device.
//!
//! The number this exists for decides whether a reranked search feels instant or hung. The retrieval
//! study measured 60 candidates at 0.14 s on a graphics card and 10.6 s on the processor, and a
//! user on a machine with no card needs to know before turning `question` on that it will take
//! seconds. `docs/embeddings.md` prints what this command found on this machine, with the commit and
//! the date, so a later change to the reranker can measure the cost again.
//!
//! Each configuration opens one session, scores a warm up call that is not counted, and then times
//! `repeats` calls of each passage count. A passage is about 900 characters, which is the size of a
//! chunk in the study. The report gives the median and the 95th percentile of each row. A percentile
//! of 20 samples is its second largest, which is why the default number of repeats is 20.

use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use inillucent_core::embed_onnx::{Device, OnnxOptions};
use inillucent_core::model::ModelManifest;
use inillucent_core::rerank_onnx::OnnxReranker;

/// Paragraphs the passages are built from. Each is about 900 characters once repeated and trimmed.
///
/// Plain English about ancient philosophy, so the tokenizer sees ordinary words. Their meaning does
/// not matter to a timing.
const SENTENCES: [&str; 8] = [
    "Thales of Miletus is often named the first philosopher because he asked what all things are made of and answered with a natural cause instead of a story about the gods.",
    "Heraclitus wrote that a person cannot step into the same river twice, because the water is always moving on and the person is never quite the same either.",
    "Socrates wrote nothing down, and what is known of his method comes from his pupils, who show him asking short questions until a confident belief turns out to contradict itself.",
    "Epicurus taught that the best life is one with as little pain as possible, and that friends, simple food and quiet thought give more lasting pleasure than wealth or fame.",
    "The Stoics held that some things are within our control and some are not, and that a calm mind comes from caring only about the first kind, which is our own judgments and choices.",
    "Aristotle founded a school at the Lyceum in Athens, where his students studied logic, biology, ethics and politics, walking in the covered colonnade as they talked.",
    "Diogenes of Sinope lived in a large jar in the marketplace and told Alexander the Great to step out of his sunlight, which the king is said to have admired.",
    "Plotinus described the world as flowing outward from a single source he called the One, and taught that the soul finds its way back by turning its attention inward.",
];

/// How many characters one timed passage has.
const PASSAGE_CHARS: usize = 900;

/// One row of the report: a configuration and a passage count, with its times.
pub struct Row {
    /// The device, such as `cpu` or `cuda:0`.
    pub device: String,
    /// The intra operator thread count, or `default` when ONNX Runtime chose.
    pub threads: String,
    /// How many passages one call scored.
    pub passages: usize,
    /// Milliseconds each timed call took, in the order they ran.
    pub samples_ms: Vec<f64>,
}

impl Row {
    /// Returns the median of the samples, in milliseconds.
    pub fn median(&self) -> f64 {
        percentile(&self.samples_ms, 50.0)
    }

    /// Returns the 95th percentile of the samples, in milliseconds.
    pub fn p95(&self) -> f64 {
        percentile(&self.samples_ms, 95.0)
    }
}

/// Returns a percentile of a set of samples, by the nearest rank method.
///
/// @param samples - the timings
/// @param share - the percentile, from 0 to 100
fn percentile(samples: &[f64], share: f64) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = ((share / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted
        .get(rank.saturating_sub(1).min(sorted.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0.0)
}

/// Builds `count` passages of about 900 characters, each starting at a different sentence.
///
/// @param count - how many passages
fn passages(count: usize) -> Vec<String> {
    (0..count)
        .map(|start| {
            let mut text = String::new();
            let mut at = start;
            while text.chars().count() < PASSAGE_CHARS {
                let sentence = SENTENCES.get(at % SENTENCES.len()).copied().unwrap_or("");
                text.push_str(sentence);
                text.push(' ');
                at += 1;
            }
            text.chars().take(PASSAGE_CHARS).collect()
        })
        .collect()
}

/// Times one configuration over each passage count.
///
/// @param dir - the reranker's model folder
/// @param device - where to run it
/// @param threads - the intra operator thread count, or `None` for ONNX Runtime's own choice
/// @param counts - the passage counts to time
/// @param repeats - how many timed calls each count gets
fn measure_one(
    dir: &Path,
    device: Device,
    threads: Option<usize>,
    counts: &[usize],
    repeats: usize,
) -> Result<Vec<Row>> {
    let manifest =
        ModelManifest::read(dir).unwrap_or_else(|_| ModelManifest::gte_reranker_modernbert_base());
    let mut options = OnnxOptions::for_model(&manifest);
    options.device = device;
    options.intra_threads = threads;
    let reranker = OnnxReranker::open_file(dir, &manifest.model_file, options)
        .with_context(|| format!("opening the reranker on {}", device.label()))?;
    let question = "which philosopher said that a person cannot step into the same river twice";
    let warm = passages(4);
    let warm_refs: Vec<&str> = warm.iter().map(String::as_str).collect();
    reranker.score(question, &warm_refs)?;
    let mut rows = Vec::new();
    for count in counts {
        let texts = passages(*count);
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let mut samples = Vec::with_capacity(repeats);
        for _ in 0..repeats {
            let started = Instant::now();
            reranker.score(question, &refs)?;
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        rows.push(Row {
            device: device.label(),
            threads: threads.map_or("default".to_string(), |count| count.to_string()),
            passages: *count,
            samples_ms: samples,
        });
    }
    Ok(rows)
}

/// Times the reranker on every device and thread count asked for, and prints a table.
///
/// @param dir - the reranker's model folder
/// @param devices - the devices, such as `cpu` and `cuda:0`
/// @param threads - the thread counts to try on the processor. A card ignores them
/// @param counts - the passage counts to time
/// @param repeats - how many timed calls each count gets
pub fn run(
    dir: &Path,
    devices: &[Device],
    threads: &[usize],
    counts: &[usize],
    repeats: usize,
) -> Result<()> {
    let mut rows: Vec<Row> = Vec::new();
    for device in devices {
        let settings: Vec<Option<usize>> = match device {
            Device::Cpu if !threads.is_empty() => threads.iter().copied().map(Some).collect(),
            _ => vec![None],
        };
        for setting in settings {
            eprintln!("measuring {} with {setting:?} threads", device.label());
            rows.extend(measure_one(dir, *device, setting, counts, repeats)?);
        }
    }
    println!("| device | threads | passages | median ms | 95th percentile ms | calls |");
    println!("|---|---:|---:|---:|---:|---:|");
    for row in &rows {
        println!(
            "| {} | {} | {} | {:.1} | {:.1} | {} |",
            row.device,
            row.threads,
            row.passages,
            row.median(),
            row.p95(),
            row.samples_ms.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nearest rank percentile of 20 samples: the median is the tenth and the 95th is the nineteenth.
    #[test]
    fn percentiles_use_the_nearest_rank() {
        let samples: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!(percentile(&samples, 50.0), 10.0);
        assert_eq!(percentile(&samples, 95.0), 19.0);
        assert_eq!(percentile(&[], 50.0), 0.0);
        assert_eq!(percentile(&[7.0], 95.0), 7.0);
    }

    /// Every passage is 900 characters and no two neighbours are the same text.
    #[test]
    fn passages_are_900_characters_and_differ() {
        let made = passages(3);
        assert!(made
            .iter()
            .all(|text| text.chars().count() == PASSAGE_CHARS));
        assert_ne!(made.first(), made.get(1));
    }
}
