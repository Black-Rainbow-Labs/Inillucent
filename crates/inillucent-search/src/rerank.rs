//! `rerank(query, passage)`: a relevance score from a cross encoder, computed inside the database.
//!
//! Invariant: **the reranker is loaded once and never guessed at.** A caller who asks for a score
//! gets one from the installed reranker or a refusal that names what is missing, never a score of
//! zero, never a score from another model, and never a silent `NULL` for a pair that had text. A
//! score is a number from 0 to 1, and a higher one means the passage answers the question better.
//!
//! One pair goes through the model per call, so scoring 60 passages this way runs the model 60
//! times. A search table that names `question` hands the reranker all of its candidates in one call
//! and groups them by length, which is much faster. This function is for scoring one pair, or a
//! handful, in an ordinary query.
//!
//! The question is passed as it is, with no prefix. `search_query: ` belongs to
//! `nomic-embed-text-v1.5` and a cross encoder was not trained with it.
//!
//! The reranker's location, its settings and its residency come from the same places the embedder's
//! do: `INILLUCENT_ONNX_DIR` and the install folder for the weights, `INILLUCENT_EMBED_THREADS` and
//! `INILLUCENT_EMBED_DEVICE` for where it runs, and `INILLUCENT_EMBED_RESIDENCY` for when it is in
//! memory.

use std::sync::{Arc, OnceLock};

use inillucent_base::DbResult;
use inillucent_core::embed_onnx::OnnxOptions;
use inillucent_core::install;
use inillucent_core::model::ModelManifest;
use inillucent_core::residency::{ManagedReranker, Residency};
use inillucent_value::Value;

use crate::embed_refusal::{cuda_would_not_start, no_reranker, reranker_would_not_run, RERANKER};

/// The managed reranker, and whether building one was possible.
///
/// A separate lock from the embedder's because the two models are different files with different
/// residency, and a query that only embeds must not load the reranker.
static RERANKER_SESSION: OnceLock<Result<Built, Failure>> = OnceLock::new();

/// A managed reranker and whether it was opened for a card.
struct Built {
    reranker: ManagedReranker,
    cuda: bool,
}

/// Why no reranker could be built.
enum Failure {
    /// No reranker is installed on this machine.
    NotInstalled,
    /// A thread count or a device setting could not be used. The text says which.
    Settings(String),
}

/// Builds the managed reranker, when this machine has one installed.
fn build() -> Result<Built, Failure> {
    let dir = install::model_dir(RERANKER).ok_or(Failure::NotInstalled)?;
    let manifest =
        ModelManifest::read(&dir).unwrap_or_else(|_| ModelManifest::gte_reranker_modernbert_base());
    let options = OnnxOptions::for_model(&manifest)
        .with_configured_machine()
        .map_err(|reason| Failure::Settings(format!("{reason:#}")))?;
    let cuda = matches!(options.device, inillucent_core::embed_onnx::Device::Cuda(_));
    Ok(Built {
        reranker: ManagedReranker::new(
            &dir,
            manifest.model_file.clone(),
            options,
            Residency::configured(),
        ),
        cuda,
    })
}

/// Turns a build failure into the refusal the caller reads.
///
/// @param failure - why no reranker exists
fn refusal_for(failure: &Failure) -> inillucent_base::DbError {
    match failure {
        Failure::NotInstalled => no_reranker(),
        Failure::Settings(reason) => reranker_would_not_run(reason),
    }
}

/// Checks that a reranker is installed and its settings can be used, without loading the weights.
///
/// A search that names `question` calls this before it runs, so a machine with no reranker refuses
/// the query with the message that names the installer, even for a search that found nothing.
pub(crate) fn available() -> DbResult<()> {
    match RERANKER_SESSION.get_or_init(build) {
        Ok(_) => Ok(()),
        Err(failure) => Err(refusal_for(failure)),
    }
}

/// Scores a question against passages with the installed reranker, in one call.
///
/// Each score is from 0 to 1 and the scores are in the order the passages were given. The pairs are
/// grouped by length under a memory ceiling, so this is much faster than scoring each passage on
/// its own.
///
/// @param query - the question, in plain words
/// @param passages - the passages to score
pub(crate) fn score(query: &str, passages: &[&str]) -> DbResult<Vec<f32>> {
    let built = match RERANKER_SESSION.get_or_init(build) {
        Ok(built) => built,
        Err(failure) => return Err(refusal_for(failure)),
    };
    built
        .reranker
        .score(query, passages)
        .map_err(|reason| match built.cuda {
            true => cuda_would_not_start("rerank", &format!("{reason:#}")),
            false => reranker_would_not_run(&format!("{reason:#}")),
        })
}

/// Returns a value's text, or nothing when it is NULL.
///
/// @param value - a SQL value
fn text_of(value: Option<&Value<'static>>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::Text(text)) => Some(String::from_utf8_lossy(text.raw()).into_owned()),
        Some(Value::Blob(blob)) => Some(String::from_utf8_lossy(blob.raw()).into_owned()),
        Some(Value::Integer(number)) => Some(number.to_string()),
        Some(Value::Real(number)) => Some(number.to_string()),
    }
}

/// `rerank(query, passage)`: the relevance of one passage to one question, from 0 to 1.
///
/// `NULL` in either argument gives `NULL`.
///
/// @param arguments - the question and the passage
fn rerank(arguments: &[Value<'static>]) -> DbResult<Value<'static>> {
    let (Some(query), Some(passage)) = (text_of(arguments.first()), text_of(arguments.get(1)))
    else {
        return Ok(Value::Null);
    };
    let scores = score(&query, &[passage.as_str()])?;
    match scores.first() {
        Some(score) => Ok(Value::Real(f64::from(*score))),
        None => Err(inillucent_base::error::refusal(
            "rerank: the reranker returned no score",
        )),
    }
}

/// Adds `rerank` to a registry.
///
/// **Deterministic and `direct_only`, for the reasons \`embed\` is.** The same pair through the same
/// weights gives the same score, so a call whose arguments do not vary in one statement may be
/// evaluated once. A function that loads a 600 MB model has no business being called from a
/// `CHECK` constraint or an index expression, where it would load the model on every insert.
///
/// @param registry - what a connection reaches functions through
pub fn register(registry: &mut inillucent_ext::registry::Registry) {
    registry.register_function(inillucent_ext::registry::UserFunction {
        flags: inillucent_ext::registry::FunctionFlags {
            deterministic: true,
            ..inillucent_ext::registry::FunctionFlags::external()
        },
        ..inillucent_ext::registry::UserFunction::external(
            "rerank",
            2,
            inillucent_ext::registry::UserBody::Scalar(Arc::new(rerank)),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rerank` is registered as a function a schema may not name, and as deterministic.
    #[test]
    fn rerank_is_registered_as_direct_only_and_deterministic() {
        let mut registry = inillucent_ext::registry::Registry::with_builtins();
        register(&mut registry);
        let flags = registry.function_flags(b"rerank");
        assert!(flags.direct_only, "`rerank` loads a 600 MB model");
        assert!(flags.deterministic);
        assert!(registry.function(b"rerank", 2).is_some());
        assert!(registry.function(b"rerank", 1).is_none());
    }

    /// `NULL` in either argument is `NULL` out, without loading anything.
    #[test]
    fn a_null_argument_gives_null() {
        assert!(matches!(
            rerank(&[Value::Null, Value::owned_text(b"text").expect("text")]),
            Ok(Value::Null)
        ));
        assert!(matches!(
            rerank(&[Value::owned_text(b"text").expect("text"), Value::Null]),
            Ok(Value::Null)
        ));
        assert!(matches!(rerank(&[]), Ok(Value::Null)));
    }
}
