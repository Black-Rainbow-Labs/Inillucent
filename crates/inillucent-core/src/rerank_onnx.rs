//! A cross encoder reranker run inside the calling process.
//!
//! A bi encoder, [`crate::embed_onnx::OnnxEmbedder`], reads a question and a passage separately and
//! a search compares the two vectors. A cross encoder reads the question and the passage together,
//! so it can see how the passage answers the question, and returns one relevance score. Running it
//! over the top rows of a first search moved nDCG@10 by +0.10 to +0.13 on every collection and
//! every embedding model in the retrieval study behind `docs/rag-explained.md`, which is the largest
//! gain that study measured.
//!
//! Invariant: **the score is the model's logit through the sigmoid function, in input order, and
//! only the passage is ever cut.** [`OnnxReranker::score`] returns `1 / (1 + e^-logit)` for each
//! passage, so a score is a number from 0 to 1 and a higher one is more relevant. A pair that is
//! longer than the token limit loses the end of its passage and never the question, because a
//! question cut in half asks a different question. The pairs cut are counted, so a caller can say
//! how many the reranker did not read in full.
//!
//! The input and output handling reproduces `tools/rag-lab/rerank.py` in the study: the tokenizer's
//! pair encoding with truncation on the second sequence only, `input_ids` and `attention_mask`
//! (and `token_type_ids` only when the graph declares that input), the first output's first column
//! as the logit.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use anyhow::{Context, Result};
use ort::session::Session;
use tokenizers::{
    EncodeInput, Tokenizer, TruncationDirection, TruncationParams, TruncationStrategy,
};

use crate::embed_onnx::{
    build_inputs, build_session, disarm_tokenizer, plan_batches, OnnxOptions, TruncationFacts,
};
use crate::model::ModelManifest;

/// The tokens a pair needs besides the passage: the special tokens at the start, between and at the end.
const SPECIAL_TOKENS: usize = 4;

/// A cross encoder that scores a question against passages.
pub struct OnnxReranker {
    session: Mutex<Session>,
    /// Encodes pairs, with truncation on the second sequence only.
    tokenizer: Tokenizer,
    /// The same tokenizer with no truncation, which measures and cuts the question on its own. A
    /// tokenizer that truncates only the second sequence refuses a single sequence.
    plain: Tokenizer,
    options: OnnxOptions,
    /// Pairs scored, and how many were cut at the token limit.
    seen: AtomicUsize,
    truncated: AtomicUsize,
    tokens: AtomicUsize,
}

impl OnnxReranker {
    /// Loads the cross encoder a model folder holds.
    ///
    /// The weights file is the one the folder's `model.json` names, or `model.onnx` when it has
    /// none.
    ///
    /// @param dir - the model folder, holding the weights, `tokenizer.json` and `model.json`
    /// @param options - the device, the thread count and the token limit
    pub fn open_model(dir: &Path, options: &OnnxOptions) -> Result<OnnxReranker> {
        let file = ModelManifest::read(dir)
            .map(|manifest| manifest.model_file)
            .unwrap_or_else(|_| "model.onnx".to_string());
        OnnxReranker::open_file(dir, &file, options.clone())
    }

    /// Loads the cross encoder from a named weights file in a model folder.
    ///
    /// @param dir - the model folder
    /// @param model_file - the weights file inside it
    /// @param options - the device, the thread count and the token limit
    pub fn open_file(dir: &Path, model_file: &str, options: OnnxOptions) -> Result<OnnxReranker> {
        let session = build_session(&dir.join(model_file), &options)?;
        let path = dir.join("tokenizer.json");
        let mut tokenizer = Tokenizer::from_file(&path)
            .map_err(|e| anyhow::anyhow!("loading {}: {e}", path.display()))?;
        disarm_tokenizer(&mut tokenizer);
        let plain = tokenizer.clone();
        // Only the passage may be cut, so a long passage never takes the question with it.
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: options.max_tokens,
                strategy: TruncationStrategy::OnlySecond,
                stride: 0,
                direction: TruncationDirection::Right,
            }))
            .map_err(|e| anyhow::anyhow!("setting the reranker's truncation: {e}"))?;
        Ok(OnnxReranker {
            session: Mutex::new(session),
            tokenizer,
            plain,
            options,
            seen: AtomicUsize::new(0),
            truncated: AtomicUsize::new(0),
            tokens: AtomicUsize::new(0),
        })
    }

    /// Returns the options this reranker was opened with.
    pub fn options(&self) -> &OnnxOptions {
        &self.options
    }

    /// How many pairs this reranker has scored and how many of them it cut at the token limit.
    pub fn truncation(&self) -> TruncationFacts {
        TruncationFacts {
            texts: self.seen.load(Ordering::Relaxed),
            truncated: self.truncated.load(Ordering::Relaxed),
            tokens: self.tokens.load(Ordering::Relaxed),
        }
    }

    /// Scores each passage against the question.
    ///
    /// Returns one number from 0 to 1 for each passage, in the order the passages were given.
    /// Higher means more relevant. The pairs are grouped by length under the memory ceiling, so
    /// scoring 60 passages in one call is much faster than 60 calls of one.
    ///
    /// @param query - the question, in plain words and with no prefix
    /// @param passages - the passages to score
    pub fn score(&self, query: &str, passages: &[&str]) -> Result<Vec<f32>> {
        if passages.is_empty() {
            return Ok(Vec::new());
        }
        let (query, query_cut) = self.fit_query(query)?;
        let pairs: Vec<EncodeInput> = passages
            .iter()
            .map(|passage| EncodeInput::Dual(query.clone().into(), (*passage).to_string().into()))
            .collect();
        let encodings = self
            .tokenizer
            .encode_batch(pairs, true)
            .map_err(|e| anyhow::anyhow!("tokenizing the pairs: {e}"))?;
        let lengths: Vec<usize> = encodings.iter().map(|e| e.get_ids().len().max(1)).collect();
        let cut = encodings
            .iter()
            .filter(|encoding| query_cut || !encoding.get_overflowing().is_empty())
            .count();
        self.seen.fetch_add(passages.len(), Ordering::Relaxed);
        self.truncated.fetch_add(cut, Ordering::Relaxed);
        self.tokens
            .fetch_add(lengths.iter().sum::<usize>(), Ordering::Relaxed);

        let mut scores = vec![0f32; passages.len()];
        for batch in plan_batches(
            &lengths,
            self.options.batch_size,
            self.options.max_batch_cells,
        ) {
            let logits = self.run_batch(&batch, &encodings)?;
            for (slot, logit) in batch.iter().zip(logits) {
                if let Some(score) = scores.get_mut(*slot) {
                    *score = sigmoid(logit);
                }
            }
        }
        Ok(scores)
    }

    /// Cuts the question when it alone would leave no room for a passage, and says whether it did.
    ///
    /// A question that fits is returned unchanged. One that does not is cut to half the token limit,
    /// so some of the passage is still read. The cut is made on tokens and written back as text, so
    /// the pair encoding sees a question that fits.
    ///
    /// @param query - the question as it was given
    fn fit_query(&self, query: &str) -> Result<(String, bool)> {
        let ids = self
            .plain
            .encode(query, false)
            .map_err(|e| anyhow::anyhow!("tokenizing the question: {e}"))?;
        let ids = ids.get_ids();
        if ids.len().saturating_add(SPECIAL_TOKENS) < self.options.max_tokens {
            return Ok((query.to_string(), false));
        }
        let keep = ids.get(..self.options.max_tokens / 2).unwrap_or(ids);
        let text = self
            .plain
            .decode(keep, true)
            .map_err(|e| anyhow::anyhow!("cutting the question: {e}"))?;
        Ok((text, true))
    }

    /// Runs one batch of pairs and returns each pair's logit, in the order of `batch`.
    ///
    /// @param batch - indices into `encodings`, all of a similar length
    /// @param encodings - the tokenizer output for the whole call
    fn run_batch(&self, batch: &[usize], encodings: &[tokenizers::Encoding]) -> Result<Vec<f32>> {
        let picked: Vec<&tokenizers::Encoding> =
            batch.iter().filter_map(|at| encodings.get(*at)).collect();
        let width = picked
            .iter()
            .map(|encoding| encoding.get_ids().len())
            .max()
            .unwrap_or(1)
            .max(1);
        let rows = picked.len();
        let mut ids = vec![0i64; rows * width];
        let mut mask = vec![0i64; rows * width];
        for (row, encoding) in picked.iter().enumerate() {
            for (col, id) in encoding.get_ids().iter().enumerate() {
                let at = row.saturating_mul(width).saturating_add(col);
                if let (Some(slot), Some(present)) = (ids.get_mut(at), mask.get_mut(at)) {
                    *slot = i64::from(*id);
                    *present = 1;
                }
            }
        }
        let types = vec![0i64; rows * width];
        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("the reranker's session lock was poisoned"))?;
        let inputs = build_inputs(&session, rows, width, ids, &mask, types)?;
        let outputs = session
            .run(inputs)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("scoring {rows} pairs of {width} tokens"))?;
        let first = outputs
            .keys()
            .next()
            .map(str::to_string)
            .context("the reranker returned no output")?;
        let (shape, data) = outputs
            .get(first.as_str())
            .context("the reranker's output vanished")?
            .try_extract_tensor::<f32>()
            .with_context(|| format!("reading the reranker's output {first}"))?;
        let columns = shape.last().copied().unwrap_or(1).max(1) as usize;
        Ok((0..rows)
            .map(|row| {
                data.get(row.saturating_mul(columns))
                    .copied()
                    .unwrap_or(0.0)
            })
            .collect())
    }
}

/// The sigmoid function, `1 / (1 + e^-x)`, which turns a logit into a score from 0 to 1.
///
/// @param logit - the raw number the model returned
pub fn sigmoid(logit: f32) -> f32 {
    1.0 / (1.0 + (-logit).exp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed_onnx::Device;

    /// The 20 pairs the study's Python reranker scored, with its logits and their sigmoids.
    ///
    /// Produced by `tools/rag-lab/rerank.py` in the retrieval study on the processor, one pair per
    /// call, from public Wikipedia text in `examples/rag-agent/corpus`.
    const REFERENCE: &str = include_str!("../fixtures/rerank_reference.json");

    /// Opens the installed reranker on the processor, or says why the test cannot run.
    fn reranker_or_skip() -> Option<OnnxReranker> {
        let Some(dir) = crate::install::model_dir(crate::install::RERANKER_MODEL) else {
            inillucent_base::testing::skipping(
                "no reranker is installed; run `inillucent setup-embeddings reranker`",
            );
            return None;
        };
        let manifest = ModelManifest::read(&dir)
            .unwrap_or_else(|_| ModelManifest::gte_reranker_modernbert_base());
        let mut options = OnnxOptions::for_model(&manifest);
        options.device = Device::Cpu;
        match OnnxReranker::open_file(&dir, &manifest.model_file, options) {
            Ok(reranker) => Some(reranker),
            Err(error) => {
                inillucent_base::testing::skipping(&format!(
                    "the reranker would not open: {error:#}. Run `inillucent setup-embeddings runtime`"
                ));
                None
            }
        }
    }

    /// One reference pair: the question, the passage, and the study's score.
    struct Pair {
        query: String,
        passage: String,
        score: f64,
    }

    /// Reads the reference pairs out of the fixture.
    fn pairs() -> Vec<Pair> {
        let document: serde_json::Value =
            serde_json::from_str(REFERENCE).expect("the fixture parses");
        document["pairs"]
            .as_array()
            .expect("the fixture has pairs")
            .iter()
            .map(|pair| Pair {
                query: pair["query"].as_str().expect("query").to_string(),
                passage: pair["passage"].as_str().expect("passage").to_string(),
                score: pair["score"].as_f64().expect("score"),
            })
            .collect()
    }

    /// The Rust reranker agrees with the study's Python scores to 1e-4 after the sigmoid, for all 20 pairs.
    ///
    /// Each pair is scored on its own, as the Python reference did, and the largest difference is
    /// printed so a later change to the graph handling shows up as a number and not only as a pass.
    #[test]
    fn scores_agree_with_the_study_python_to_1e_minus_4() {
        let Some(reranker) = reranker_or_skip() else {
            return;
        };
        let pairs = pairs();
        assert_eq!(pairs.len(), 20);
        let mut largest = 0f64;
        for pair in &pairs {
            let scored = reranker
                .score(&pair.query, &[pair.passage.as_str()])
                .expect("scores");
            let difference = (f64::from(scored[0]) - pair.score).abs();
            largest = largest.max(difference);
            assert!(
                difference < 1e-4,
                "{:?} scored {} and the study's Python scored {}",
                pair.query,
                scored[0],
                pair.score
            );
        }
        eprintln!("largest difference from the study's Python scores: {largest:e}");
    }

    /// Scoring all 20 passages of one question in a batch gives the same scores as one at a time.
    ///
    /// The batch pads short pairs to the longest, and a padded pair must not score differently.
    #[test]
    fn a_batch_scores_the_same_as_single_calls() {
        let Some(reranker) = reranker_or_skip() else {
            return;
        };
        let pairs = pairs();
        let query = pairs[0].query.clone();
        let passages: Vec<&str> = pairs.iter().map(|pair| pair.passage.as_str()).collect();
        let batched = reranker.score(&query, &passages).expect("scores the batch");
        assert_eq!(batched.len(), 20);
        for (at, passage) in passages.iter().enumerate() {
            let alone = reranker.score(&query, &[passage]).expect("scores alone")[0];
            assert!(
                (batched[at] - alone).abs() < 1e-4,
                "passage {at} scored {} in a batch and {alone} alone",
                batched[at]
            );
        }
    }

    /// A relevant passage scores above an unrelated one, every score is from 0 to 1, and an empty list scores nothing.
    #[test]
    fn a_relevant_passage_outscores_an_unrelated_one() {
        let Some(reranker) = reranker_or_skip() else {
            return;
        };
        let pairs = pairs();
        let query = &pairs[0].query;
        let relevant = reranker
            .score(query, &[pairs[0].passage.as_str()])
            .expect("scores")[0];
        let unrelated = reranker
            .score(query, &[pairs[1].passage.as_str()])
            .expect("scores")[0];
        assert!(relevant > unrelated, "{relevant} against {unrelated}");
        assert!((0.0..=1.0).contains(&relevant) && (0.0..=1.0).contains(&unrelated));
        assert!(reranker
            .score(query, &[])
            .expect("scores nothing")
            .is_empty());
    }

    /// A passage over the token limit is cut and counted, and the question is never cut.
    #[test]
    fn a_long_passage_is_cut_at_the_limit_and_counted() {
        let Some(reranker) = reranker_or_skip() else {
            return;
        };
        let long = "word ".repeat(3000);
        let before = reranker.truncation();
        let scores = reranker
            .score("what is the word", &[long.as_str(), "a short one"])
            .expect("scores");
        assert_eq!(scores.len(), 2);
        let after = reranker.truncation();
        assert_eq!(after.texts - before.texts, 2);
        assert_eq!(
            after.truncated - before.truncated,
            1,
            "only the long passage was cut"
        );
    }

    /// A question longer than the token limit is cut, counted, and still scored.
    #[test]
    fn a_question_over_the_limit_is_cut_and_the_pair_is_counted() {
        let Some(reranker) = reranker_or_skip() else {
            return;
        };
        let question = "why ".repeat(1500);
        let before = reranker.truncation();
        let scores = reranker
            .score(&question, &["a short passage"])
            .expect("scores");
        assert_eq!(scores.len(), 1);
        assert!(scores[0].is_finite());
        assert_eq!(reranker.truncation().truncated - before.truncated, 1);
    }

    /// The sigmoid of 0 is one half, and large logits saturate without overflowing.
    #[test]
    fn the_sigmoid_maps_a_logit_into_zero_to_one() {
        assert_eq!(sigmoid(0.0), 0.5);
        assert!(sigmoid(20.0) > 0.999_999);
        assert!(sigmoid(-20.0) < 0.000_001);
        assert!(sigmoid(1000.0).is_finite());
        assert!(sigmoid(-1000.0).is_finite());
        assert!(sigmoid(2.0) > sigmoid(1.0));
    }
}
