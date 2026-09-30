//! What loading and dropping the embedding model on a card leaves on that card.
//!
//! `docs/embeddings.md` says an idle model's memory comes back. On a processor
//! that is true. On a card it was measured not to be, from Nikaya: a reranker on
//! `cuda:1` loaded and unloaded twice, the residency counters said "not in
//! memory", and about 500 MB stayed on the card for the life of the process.
//!
//! There are two candidates for what stays, and they need different answers:
//!
//! | candidate | owned by | released when |
//! |---|---|---|
//! | the CUDA execution provider's memory arena | the session | the session is dropped |
//! | the CUDA primary context, with the kernels loaded into it | the process | `cudaDeviceReset`, or process exit |
//!
//! So this module loads the model, embeds a batch of documents through it so
//! the arena grows the way it does in real use, drops it, and does that several
//! times, reading the card's memory after every step. If the arena were kept,
//! the residue would grow with the batch. If a session leaked, the residue
//! would grow with each cycle. What was measured is neither: the residue is a
//! constant set by the first session, and a process that only creates a CUDA
//! context and does nothing else costs the same amount.
//!
//! The number is the card's whole memory as `nvidia-smi` reports it, not this
//! process's share, because Windows does not report a per-process figure under
//! the WDDM driver. A card nothing else is using between samples is therefore
//! part of the method, and the report prints the starting figure so a reader can
//! see what else was there.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use inillucent_core::embed::Embedder;
use inillucent_core::embed_onnx::{Device, OnnxEmbedder, OnnxOptions};
use inillucent_core::model::ModelManifest;

/// One reading of the card.
pub struct Phase {
    /// What had just happened when the card was read.
    pub name: String,
    /// The card's memory in use, in MiB.
    pub used_mib: u64,
}

/// What one run of the study asked for.
pub struct MemoryStudy<'a> {
    /// The model directory.
    pub dir: &'a Path,
    /// The model contract, which fixes the prefixes, the pooling and the width.
    pub manifest: &'a ModelManifest,
    /// The card, by its ordinal.
    pub card: i32,
    /// Load, embed and drop cycles.
    pub cycles: usize,
    /// Documents embedded per cycle, which is what grows the arena.
    pub documents: usize,
    /// How long to wait before each reading.
    pub settle: Duration,
}

/// Reads one card's memory in use, in MiB.
///
/// Through `nvidia-smi` rather than the CUDA runtime, because asking the CUDA
/// runtime creates a CUDA context in this process, and the context is one of the
/// two things being measured.
///
/// @param card - the card's ordinal
pub fn card_used_mib(card: i32) -> Result<u64> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "-i",
            &card.to_string(),
            "--query-gpu=memory.used",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .context("running nvidia-smi")?;
    anyhow::ensure!(
        output.status.success(),
        "nvidia-smi failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .parse::<u64>()
        .with_context(|| format!("reading nvidia-smi's answer {:?}", text.trim()))
}

/// The documents every cycle embeds.
///
/// Long ones, so each batch asks the arena for real attention buffers. A short
/// query would grow the arena by a few megabytes and could not show whether the
/// arena comes back.
///
/// @param count - how many documents
fn documents(count: usize) -> Vec<String> {
    let paragraph = "The quarterly review covered the migration of the billing service, \
        the two outages in March and what caused them, the budget for the new storage \
        cluster, and the hiring plan for the platform team. ";
    (0..count)
        .map(|i| format!("Document {i}. {}", paragraph.repeat(12)))
        .collect()
}

/// Runs the study and returns every reading, in order.
///
/// @param study - the model, the card and how many cycles
pub fn measure(study: &MemoryStudy<'_>) -> Result<Vec<Phase>> {
    let read = |name: String| -> Result<Phase> {
        std::thread::sleep(study.settle);
        Ok(Phase {
            name,
            used_mib: card_used_mib(study.card)?,
        })
    };
    let texts = documents(study.documents);
    let mut phases = vec![read("before any session".to_string())?];
    for cycle in 1..=study.cycles {
        let options = OnnxOptions {
            device: Device::Cuda(study.card),
            ..OnnxOptions::for_model(study.manifest)
        };
        let embedder = OnnxEmbedder::open_model(study.dir, &study.manifest.model_file, options)
            .with_context(|| format!("opening {} on cuda:{}", study.dir.display(), study.card))?;
        phases.push(read(format!("cycle {cycle}: session open"))?);
        let vectors = embedder.embed_documents(&texts)?;
        anyhow::ensure!(
            vectors.len() == texts.len(),
            "embedded {} of {} documents",
            vectors.len(),
            texts.len()
        );
        phases.push(read(format!(
            "cycle {cycle}: {} documents embedded",
            texts.len()
        ))?);
        drop(embedder);
        phases.push(read(format!("cycle {cycle}: session dropped"))?);
    }
    Ok(phases)
}

/// What stays on the card after the last session is dropped, in MiB.
///
/// @param phases - the readings, the first of which is the starting figure
pub fn residue_mib(phases: &[Phase]) -> i64 {
    match (phases.first(), phases.last()) {
        (Some(first), Some(last)) => last.used_mib as i64 - first.used_mib as i64,
        _ => 0,
    }
}

/// Prints the readings, in markdown, on standard output.
///
/// @param phases - the readings, the first of which is the starting figure
pub fn report(phases: &[Phase]) {
    let Some(first) = phases.first() else {
        return;
    };
    println!("| moment | card MiB | against the start |");
    println!("|---|---:|---:|");
    for phase in phases {
        println!(
            "| {} | {} | {:+} |",
            phase.name,
            phase.used_mib,
            phase.used_mib as i64 - first.used_mib as i64
        );
    }
    println!();
    println!(
        "Left on the card after the last drop: {:+} MiB.",
        residue_mib(phases)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The residue is the last reading against the first, whatever happened
    /// in between.
    #[test]
    fn the_residue_is_the_last_reading_against_the_first() {
        let phases = vec![
            Phase {
                name: "start".to_string(),
                used_mib: 1000,
            },
            Phase {
                name: "open".to_string(),
                used_mib: 2400,
            },
            Phase {
                name: "dropped".to_string(),
                used_mib: 1495,
            },
        ];
        assert_eq!(residue_mib(&phases), 495);
        assert_eq!(residue_mib(&[]), 0);
    }

    /// Every document is long enough to ask for a real attention buffer.
    #[test]
    fn the_study_documents_are_long() {
        let texts = documents(3);
        assert_eq!(texts.len(), 3);
        assert!(texts.iter().all(|t| t.len() > 2000));
    }
}
