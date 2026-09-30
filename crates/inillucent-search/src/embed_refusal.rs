//! Why `embed(TEXT)` could not answer on a machine that is not set up for it.
//!
//! Invariant: **the sentence a person reads is built here, and it is built in a
//! part of the crate that a default build compiles.**
//!
//! ## Why these two functions are not in `embed.rs`
//!
//! `embed.rs` is behind the `embed` feature, because the function it registers
//! links a native machine-learning runtime. That is right for the function. It
//! was wrong for the refusals, and task-1952 is the bill for it.
//!
//! The refusal `embed.rs` had was built with `inillucent_base::error::misuse`,
//! which puts what it is given into the diagnostic detail rather than into the
//! message - so `DbError::message()` answered `SQLITE_MISUSE`'s manifest text
//! and the published 0.1.2 archives told a person with no model installed
//! `Error [syntax]: bad parameter or other API misuse`. `embed.rs` had a test
//! asserting the opposite, and that test was correct and would have failed. It
//! never ran: the module is behind a feature, every crate that has that feature
//! has it off by default, and `tests/selection.toml` has no way to ask for one,
//! so `inillucent-testrun` builds the `inillucent-search` lib target with
//! default features and the test is not in the binary.
//!
//! Nothing about a sentence needs ONNX. Moving the two that matter out from
//! behind the gate puts them in the lib target the runner already builds, and
//! their tests below run on every `--tier retrieval` and every `--changed` that
//! touches this crate.

use inillucent_base::error::{self, DbError};
use inillucent_core::install;

/// The model `embed(TEXT)` embeds with, named in the refusal so a person can
/// see which weights the command is about to download.
pub const MODEL: &str = install::DEFAULT_MODEL;

/// The refusal a machine with no embedding model installed gets.
///
/// It names the command that fixes it rather than the variable that would work
/// around it, because a person reading this has almost always never run the
/// installer, and the command is one line.
///
/// **`unmet_requirement` and not `misuse`, which is the whole of task-1952.**
/// The marker it sets is what makes `inillucent-driver` answer the status
/// `invalid_state` instead of `syntax`: the statement is valid SQL, the engine
/// built the function, and this machine has not got the weights yet.
pub fn no_model() -> DbError {
    error::unmet_requirement(
        "an embedding model",
        format!(
            "embed: no embedding model is installed. Run `inillucent setup-embeddings` to \
             download {MODEL} and the ONNX Runtime it needs, or set {} to a directory that \
             already holds them",
            install::MODEL_DIR_VAR
        ),
    )
}

/// The refusal a machine gets when a model is installed and would not run.
///
/// The usual cause is the ONNX Runtime: the weights and the runtime are two
/// separate downloads, `inillucent setup-embeddings runtime` installs the
/// second on its own, and a machine that has the first and not the second
/// arrives here rather than at [`no_model`]. So this names the runtime as the
/// thing to install, and says "the usual cause" because it might be something
/// else.
///
/// **The runtime's own words are the detail and not the message.** `ort` says
/// what it could not load by naming the file, and `inillucent-base` documents a
/// message as holding no path - so the reason stays inside the process, where a
/// caller that opened the database with diagnostics on can read it.
///
/// @param reason - what the embedder said, for the diagnostic detail
pub fn model_would_not_run(reason: &dyn std::fmt::Display) -> DbError {
    error::unmet_requirement(
        "a working ONNX Runtime",
        "embed: an embedding model is installed but did not run. The usual cause is a missing \
         ONNX Runtime, which `inillucent setup-embeddings runtime` installs; open the database \
         with diagnostics on to read what it said",
    )
    .with_detail(format!("embed: {reason}"))
}

/// The reranker `rerank()` and a reranked search score with, named in the refusal so a person can
/// see which weights the command is about to download.
pub const RERANKER: &str = install::RERANKER_MODEL;

/// The refusal a machine with no reranker installed gets.
///
/// It names the command that installs the reranker and how large the download is, because the
/// reranker is a separate 600 MB download that `setup-embeddings all` does not fetch. It is marked
/// as a missing component, so the driver answers `invalid_state`, exit code 1: the statement is
/// fine, the function is built, and this machine has not got the weights yet.
pub fn no_reranker() -> DbError {
    error::unmet_requirement(
        "a reranker model",
        format!(
            "rerank: no reranker is installed. Run `inillucent setup-embeddings reranker` to \
             download {RERANKER} (about 600 MB), or set {} to a directory that already holds it",
            install::MODEL_DIR_VAR
        ),
    )
}

/// The refusal a reranker that is installed and would not run gets.
///
/// @param reason - what the reranker said, for the diagnostic detail
pub fn reranker_would_not_run(reason: &dyn std::fmt::Display) -> DbError {
    error::unmet_requirement(
        "a working ONNX Runtime",
        "rerank: a reranker is installed but did not run. The usual cause is a missing ONNX \
         Runtime, which `inillucent setup-embeddings runtime` installs; open the database with \
         diagnostics on to read what it said",
    )
    .with_detail(format!("rerank: {reason}"))
}

/// The refusal a build without embedding support gives for a function or a column that needs it.
///
/// Marked `unsupported`, so the driver answers `unsupported` and the command line exits with 3: the
/// statement is spelled correctly and this build has not got the feature, which is a different
/// answer from a machine that has the feature and not the weights.
///
/// @param what - the function or column, such as `rerank(TEXT, TEXT)`
pub fn not_built(what: &str) -> DbError {
    error::refusal(format!(
        "{what}: this build has no embedding support compiled in"
    ))
    .with_unsupported(what)
}

/// The refusal a search that names `question` and no candidates gets.
///
/// A reranker reorders the rows a search found, so a search with neither a `MATCH` nor a `vector`
/// constraint has nothing to reorder.
pub fn nothing_to_rerank() -> DbError {
    error::unmet_requirement(
        "a search to rerank",
        "inillucent_search: question needs candidates to rerank. Add a MATCH constraint, a vector \
         constraint, or both, so the search finds rows for the reranker to reorder",
    )
}

/// The refusal a call gets when the CUDA device was asked for and did not start.
///
/// **It never becomes a run on the processor.** A bulk embedding meant to take
/// twenty minutes on a card takes about twelve hours on the processor, and nothing
/// in the output would say why. The message names the command that installs the
/// runtime build that carries CUDA, which is the usual missing piece.
///
/// @param function - the SQL function or command that was called, such as `embed`
/// @param reason - what the runtime said, for the diagnostic detail
pub fn cuda_would_not_start(function: &str, reason: &dyn std::fmt::Display) -> DbError {
    error::unmet_requirement(
        "a working CUDA runtime",
        format!(
            "{function}: the CUDA device was asked for and did not start. Run `inillucent \
             setup-embeddings runtime --gpu` to install the ONNX Runtime build that carries \
             CUDA, and check that the CUDA toolkit and cuDNN are installed. The call was not run \
             on the processor instead. Set {} to cpu to use the processor",
            install::DEVICE_VAR
        ),
    )
    .with_detail(format!("{function}: {reason}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusal names the command that installs the model, in the field a
    /// caller actually reads.
    ///
    /// **It asserts on `Display` on purpose.** `Display` writes
    /// `DbError::message()`, and `message()` is what `inillucent-driver`, the
    /// command line, `inillucent-mcp` and every language binding report. The
    /// same assertion in `embed.rs` was true and never ran; this one runs in a
    /// default build.
    #[test]
    fn the_refusal_names_the_command_that_fixes_it() {
        let message = format!("{}", no_model());
        assert!(message.contains("inillucent setup-embeddings"), "{message}");
        assert!(message.contains(MODEL), "{message}");
        assert!(message.contains(install::MODEL_DIR_VAR), "{message}");
    }

    /// It is marked as a component this machine has not got, which is what the
    /// driver reads to answer `invalid_state` rather than `syntax`.
    ///
    /// The marker and not the wording, so that improving the sentence cannot
    /// change the status - the argument the `unsupported` marker beside it was
    /// added on.
    #[test]
    fn the_refusal_is_marked_as_a_missing_component_and_not_as_an_unbuilt_one() {
        let refused = no_model();
        assert_eq!(refused.requirement(), Some("an embedding model"));
        assert_eq!(
            refused.unsupported(),
            None,
            "embed is built; this machine has not got the weights"
        );
    }

    /// A model that will not run is its own refusal, and the runtime's words
    /// stay inside the process.
    ///
    /// Both halves matter. Naming the runtime is what turns "bad parameter or
    /// other API misuse" into a sentence a person can act on, and keeping
    /// `ort`'s text out of the message is what `inillucent-base` requires of
    /// anything a caller is shown: it names the file it could not open.
    #[test]
    fn a_model_that_will_not_run_names_the_runtime_and_keeps_the_path_inside() {
        let refused =
            model_would_not_run(&"cannot open /home/j/.inillucent/runtime/libonnxruntime.so");
        let message = format!("{refused}");
        assert!(message.contains("setup-embeddings runtime"), "{message}");
        assert!(!message.contains("libonnxruntime"), "{message}");
        assert!(
            refused
                .detail()
                .is_some_and(|it| it.contains("libonnxruntime")),
            "the runtime's own words are kept for a diagnostic reader"
        );
        assert_eq!(refused.requirement(), Some("a working ONNX Runtime"));
    }

    /// A CUDA start that failed names the command that installs the CUDA runtime, and says it did
    /// not fall back to the processor.
    #[test]
    fn a_cuda_refusal_names_the_gpu_runtime_and_the_absence_of_a_fallback() {
        let refused = cuda_would_not_start("embed", &"no provider");
        let message = format!("{refused}");
        assert!(
            message.contains("setup-embeddings runtime --gpu"),
            "{message}"
        );
        assert!(message.contains("not run on the processor"), "{message}");
        assert_eq!(refused.requirement(), Some("a working CUDA runtime"));
    }

    /// The reranker refusals name what to run, and are marked so the driver reports them correctly.
    #[test]
    fn the_reranker_refusals_name_their_fix_and_carry_the_right_marker() {
        let missing = no_reranker();
        let message = format!("{missing}");
        assert!(
            message.contains("inillucent setup-embeddings reranker"),
            "{message}"
        );
        assert!(message.contains(RERANKER), "{message}");
        assert_eq!(missing.requirement(), Some("a reranker model"));
        assert_eq!(missing.unsupported(), None);

        let unbuilt = not_built("rerank(TEXT, TEXT)");
        assert_eq!(unbuilt.unsupported(), Some("rerank(TEXT, TEXT)"));
        assert!(format!("{unbuilt}").contains("no embedding support compiled in"));

        let nothing = nothing_to_rerank();
        assert!(format!("{nothing}").contains("MATCH"));
        assert_eq!(nothing.requirement(), Some("a search to rerank"));

        let broken = reranker_would_not_run(&"boom");
        assert!(format!("{broken}").contains("setup-embeddings runtime"));
        assert!(!format!("{broken}").contains("boom"));
    }

    /// Neither refusal is the other, so a caller told to install the runtime is
    /// not a caller told to install the weights.
    #[test]
    fn the_two_refusals_ask_for_different_things() {
        assert_ne!(
            no_model().requirement(),
            model_would_not_run(&"x").requirement()
        );
        assert!(!format!("{}", model_would_not_run(&"x")).contains(MODEL));
    }
}
