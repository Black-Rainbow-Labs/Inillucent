//! Turning a manifest of immutable segments and a bounded delta log into the
//! one index a query is answered from.
//!
//! Invariant: a query is answered by **one** index, never by several lists
//! that were scored separately and glued together. That is not a convenience.
//! BM25 scores are relative to a corpus - the inverse document frequency of a
//! term is a property of the whole collection - so a hit scored against one
//! segment and a hit scored against another are two numbers on two different
//! scales, and ordering them together by raw score produces a ranking that is
//! wrong in a way no test on either half would catch.
//!
//! That rules out the shape a segmented vector store usually takes - fan a
//! query out to every segment, let each answer its own top k, merge the
//! merged lists by score - for the lexical branch, and this module does not
//! take it even for the vector branch, so that one codepath answers both. The
//! segments a commit now writes (see `store::SegmentMeta`) are a **storage**
//! device: they bound what a write has to serialise, not what a query has to
//! score against. A query still folds every live segment into one logical
//! index before it runs, the same `replace_document`/`tombstone` calls the
//! existing engine already uses for a live sync, oldest segment first so a
//! newer edit of the same row always lands last and therefore wins. The
//! result is scored exactly as if it had been built from one pass over every
//! row - because inserting the newest few thousand rows into an
//! already-built index is a small amount of the same graph work committing
//! from scratch would do, not a different, cheaper approximation of it -
//! and it is cached, keyed by which segments and which delta entries produced
//! it, so a second query at the same snapshot pays nothing and a query after
//! one insert pays one append.
//!
//! Publishing a new segment is the write side, and there are three ways it
//! happens:
//!
//! - **A commit whose delta log has passed its threshold flushes.** The
//!   pending batch is built into a brand new segment from just those rows -
//!   never by loading and rewriting an existing one - so a flush's cost is the
//!   batch, never the corpus. This is `SearchTable::flush` in `module.rs`.
//! - **A level that has accumulated enough segments merges.** Several small
//!   segments are folded into one bigger one at the next level, by the same
//!   incremental replay a query already does, so a merge costs the segments
//!   being merged and nothing older. This is `SearchTable::merge_cascade`,
//!   and it is itself bounded: a commit folds at most
//!   `Options::merge_budget_chunks` chunks' worth before checkpointing what
//!   it has done and leaving the rest for a later commit
//!   (`SearchTable::continue_merge`), unless the level has fallen far enough
//!   behind to trip `Options::crisis_at`, in which case one commit pays for
//!   the whole thing rather than let the query-time fold keep growing.
//! - **`compact` and `rebuild` build in one pass** over every row, discarding
//!   every existing segment for one fresh one. This is what removes a
//!   tombstoned chunk for good; folding and merging never do, because both
//!   only ever append.
//!
//! Automatic compaction used to build in one pass on every commit that crossed
//! the threshold, which meant one ordinary `INSERT` could pay a full graph
//! build - nine and a half minutes on the 598,560 chunk corpus this engine is
//! deployed on, in the middle of somebody else's transaction (task-1894). That
//! was fixed by folding pending deltas into the existing generation instead of
//! rebuilding it, which bounded the *graph* work; it left the *bytes* written
//! by a publish proportional to the corpus, because a generation was one
//! serialised index and publishing meant rewriting the whole thing however few
//! rows changed (`docs/roadmap.md` item 10). Segments are what bounds that
//! too: a flush's bytes are the batch's own segment, never the whole file -
//! and once a constant flush cadence made flushing frequent enough that a
//! whole level's worth of segments could need merging inside one commit, the
//! merge bound above is what keeps *that* commit from paying for a level
//! several batches wide in one go.

use std::collections::BTreeSet;
use std::sync::Mutex;

use inillucent_base::DbResult;
use inillucent_core::filter::{AttributeFilter, Filter};
use inillucent_core::index::{Index, IndexConfig};
use inillucent_core::rank::{AdaptiveWeights, Fusion, HitOrigin, RRF_K};
use inillucent_core::store::ChunkInput;
use inillucent_ext::vtab::{failure, Context};

use crate::options::{FusionChoice, Metric, Mode, Options};
use crate::store::{state, Delta, Op, Row, SegmentMeta, Store};

/// The source name every row of a search table is filed under.
///
/// `inillucent_core` files a chunk under a source and an external document id, and
/// the filter language selects on the source. A SQL search table is one source
/// with one document per row, so the name is fixed and the row id is the
/// document; a caller who wants several logical corpora makes several tables,
/// which is what a SQL schema is for.
pub const SOURCE: &str = "inillucent_search";

/// The widest traversal the recall control will ask for.
///
/// Beyond this an approximate search is doing more work than the exhaustive
/// scan it was chosen instead of, so a caller asking for more is asking for the
/// exact path and gets it.
const MAX_OVERSAMPLE: f32 = 64.0;

/// One hit, as the module reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// The rowid of the row that matched.
    pub id: i64,
    /// The fused score, which orders the list.
    pub score: f32,
    /// How good the hit is in absolute terms, in `[0, 1]`.
    pub confidence: f32,
    /// Which branch or branches produced it.
    pub origin: HitOrigin,
}

/// What a caller asked the index for.
#[derive(Clone, Debug, Default)]
pub struct Request {
    /// The query text, when there is a lexical branch to run.
    pub text: Option<String>,
    /// The query vector, when there is a vector branch to run.
    pub vector: Vec<f32>,
    /// How many hits to return.
    pub limit: usize,
    /// The recall target, between zero and one, or nothing for the default.
    pub recall: Option<f32>,
    /// The question in plain words, when the statement named `question` and so asked for the
    /// rows to be reranked. It is not part of what the keyword and vector branches search for.
    pub question: Option<String>,
    /// The facet constraints the statement carried, as the facet's column
    /// position and the value that column must hold.
    ///
    /// Positions rather than names because that is what the access path can
    /// record in one character, and the name is read back from the declaration
    /// where the value has to agree with what the build filed it under.
    pub facets: Vec<(usize, String)>,
}

/// The index one connection is currently answering from.
#[derive(Debug, Default)]
pub struct Cache {
    inner: Mutex<Option<Cached>>,
}

/// What the cache is holding, and exactly which state it describes.
struct Cached {
    /// Which segments, in fold order, the cached index was built from.
    ///
    /// Compared against the live manifest on every query rather than trusted:
    /// a flush or a merge changes this list, and a cache that kept answering
    /// from the segments it was built over would be answering from a set a
    /// newer commit has already superseded.
    segments: Vec<i64>,
    /// The delta entries folded in on top of `segments`, in order.
    applied: Vec<Delta>,
    rows: usize,
    index: Index,
}

impl std::fmt::Debug for Cached {
    /// Reports what the cached index covers, since an index cannot print itself.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Cached")
            .field("segments", &self.segments)
            .field("applied", &self.applied.len())
            .field("rows", &self.rows)
            .finish()
    }
}

impl Cache {
    /// Returns an empty cache.
    pub fn new() -> Cache {
        Cache {
            inner: Mutex::new(None),
        }
    }

    /// Forgets whatever is cached.
    ///
    /// Called when a transaction ends, because the cheapest correct thing to do
    /// with an index built from rows that may have just been undone is to stop
    /// holding it. The content-addressed key would catch it anyway; this only
    /// stops the memory being held until the next query notices.
    pub fn forget(&self) {
        if let Ok(mut held) = self.inner.lock() {
            *held = None;
        }
    }

    /// Keeps the cached index after this connection's own flush moved delta entries into a segment.
    ///
    /// A flush writes a new segment from exactly the delta entries above the covered mark and
    /// removes those entries from the log. A cached index built over the segments before the
    /// flush plus those same entries holds the same live rows as one built over the segments
    /// after it, so only its key changes. Without this, the first search after every commit that
    /// crossed the flush threshold loaded every segment again: 36 ms on 5,000 rows of 384
    /// numbers, against well under a millisecond for the search itself.
    ///
    /// @param before - the live segment ids before the flush, sorted
    /// @param flushed - the delta entries the flush wrote into the new segment, in log order
    /// @param after - the live segment ids after the flush, sorted
    pub fn after_flush(&self, before: &[i64], flushed: &[Delta], after: Vec<i64>) {
        let Ok(mut held) = self.inner.lock() else {
            return;
        };
        match held.as_mut() {
            Some(cached) if cached.segments == before && cached.applied == flushed => {
                cached.segments = after;
                cached.applied = Vec::new();
            }
            _ => *held = None,
        }
    }

    /// Keeps the cached index after a finished merge replaced some segments with one.
    ///
    /// A merge folds its inputs into one segment that holds the same live rows, and it does not
    /// touch the delta log, so a cached index over the inputs is an index over the result.
    ///
    /// @param before - the live segment ids before the merge, sorted
    /// @param after - the live segment ids after the merge, sorted
    pub fn after_merge(&self, before: &[i64], after: Vec<i64>) {
        let Ok(mut held) = self.inner.lock() else {
            return;
        };
        match held.as_mut() {
            Some(cached) if cached.segments == before => cached.segments = after,
            _ => *held = None,
        }
    }

    /// Answers one search, refreshing the merged index first.
    pub fn search(
        &self,
        context: &mut Context<'_>,
        store: &Store,
        options: &Options,
        request: &Request,
    ) -> DbResult<Vec<Hit>> {
        let mut held = self
            .inner
            .lock()
            .map_err(|_| failure("inillucent_search: the index cache is poisoned"))?;
        refresh(&mut held, context, store, options)?;
        let Some(cached) = held.as_ref() else {
            return Ok(Vec::new());
        };
        run(&cached.index, options, request)
    }

    /// Returns how many live rows the merged index holds.
    pub fn live_rows(
        &self,
        context: &mut Context<'_>,
        store: &Store,
        options: &Options,
    ) -> DbResult<usize> {
        let mut held = self
            .inner
            .lock()
            .map_err(|_| failure("inillucent_search: the index cache is poisoned"))?;
        refresh(&mut held, context, store, options)?;
        Ok(held.as_ref().map(|cached| cached.rows).unwrap_or(0))
    }
}

/// Returns the index configuration a declaration implies.
///
/// Everything about ranking - the fusion method and its weight, the coverage
/// exponent, proximity, phrase weighting, the adaptive weighting - is left at
/// the measured defaults `inillucent_core` ships, because those are the settings
/// the existing quality scorecard was produced with and this phase is required
/// not to change what the reader answers. What the declaration decides is the
/// shape: how wide a vector is, and whether the vector branch is allowed to
/// approximate. The one ranking setting a declaration may name is a fixed
/// `vector_weight`, which replaces the adaptive weight for that table only;
/// see `Options::vector_weight` for what it was measured against.
/// @param options - the table's declaration
pub fn configuration(options: &Options) -> IndexConfig {
    let mut config = IndexConfig {
        // A lexical-only table still has a vector set, because every structure
        // underneath is indexed by chunk ordinal and a zero-width one would
        // have to be special-cased in all of them. One dimension of zero costs
        // four bytes a row and keeps every path identical.
        dims: options.dims.max(1),
        ..IndexConfig::default()
    };
    match options.mode {
        // `exhaustive_below = usize::MAX` is `inillucent_core`'s own way of saying
        // "never traverse": every vector search compares every candidate, which
        // is the only setting that is correct by construction and is what an
        // exact table promises.
        //
        // An exact table never walks its graph, so it gets the same two link graph a lexical
        // only table gets below: the structures stay indexed by chunk ordinal and `append` still
        // finds a graph to extend, but a flush no longer spends most of its time linking nodes
        // nobody visits. Measured on 5,000 rows of 384 numbers inserted in one transaction, the
        // graph build was 58% of the processor time of the insert. A table cannot change its
        // mode after it is created, and a `m` or `ef_construction` the declaration names still
        // applies, because those are set after this.
        Mode::Exact => {
            config.hnsw.exhaustive_below = usize::MAX;
            config.hnsw.m = 2;
            config.hnsw.ef_construction = 2;
        }
        Mode::Approximate => {}
    }
    if !options.has_vectors() {
        // The graph is never consulted on a lexical-only table, so building a
        // well-connected one is work with nothing to show for it. It is still
        // built, because every structure is indexed by chunk ordinal and an
        // absent graph would make `append` fall back to a full rebuild.
        config.hnsw.m = 2;
        config.hnsw.ef_construction = 2;
        config.hnsw.exhaustive_below = usize::MAX;
    }
    // **The graph is built on every core this machine has.**
    //
    // `HnswParams::build_threads` defaults to one, and that default is right
    // where it lives: `inillucent-core`'s own gradings compare settings, and a
    // graph that came out differently because the scheduler interleaved two
    // inserts differently would make every comparison a comparison of two
    // things at once. A *store* is not a grading. It is rebuilt whenever
    // compaction runs, over a corpus of whatever size the application has, and
    // the sequential build of the 598,560-chunk mailbox this engine is deployed
    // on takes nine and a half minutes on one core of twenty-four.
    //
    // The parallel build was written for exactly this and was never switched
    // on: a lock per adjacency list, neighbours merged rather than assigned so
    // a concurrent insert's back-edge is not erased, which is what hnswlib and
    // FAISS do. The graph it produces is valid and is not the sequential one -
    // the same property an approximate index has anyway.
    //
    // One thread when the machine will not say how many it has, which is the
    // old behaviour rather than a guess.
    config.hnsw.build_threads = options.threads.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
    });
    // **The three graph parameters an index may have named.** `WITH (m = 32,
    // ef_construction = 128)` on a `CREATE INDEX ... USING inillucent_hnsw`
    // reaches here, which is the only place they mean anything: a graph is
    // built once and then searched, and both halves read these. An index that
    // named none of them leaves the build's own defaults, which is what every
    // store had before the parameters existed.
    if let Some(m) = options.m {
        config.hnsw.m = m;
    }
    if let Some(ef) = options.ef_construction {
        config.hnsw.ef_construction = ef;
    }
    if let Some(ef) = options.ef_search {
        config.hnsw.ef_search = ef;
    }
    // **The one line this whole ticket is about.** Everything above decides
    // how the graph is shaped; this decides what it is a graph *of*. Cosine
    // needs every stored vector normalized to unit length; L2 needs the
    // opposite, the raw magnitude kept - so the metric has to reach
    // `inillucent_core::vectors::VectorSet::push` before the first row is
    // ever inserted, not only the search call at the end. `IndexConfig::metric`
    // is the field that carries it there.
    config.metric = core_metric(options.metric);
    if let Some(fixed) = fixed_fusion(options) {
        config.fusion = fixed;
        config.adaptive_fusion = false;
    }
    config
}

/// Returns the fusion a declaration fixes, or `None` when the adaptive default applies.
///
/// `rrf` gives reciprocal rank fusion with the damping constant the retrieval
/// engine ships, and a declared `vector_weight` gives min-max scaling at that
/// weight. Neither is adaptive, so the caller turns adaptive weighting off
/// whenever this returns a fusion.
///
/// @param options - the table's declaration
fn fixed_fusion(options: &Options) -> Option<Fusion> {
    match options.fusion {
        FusionChoice::Adaptive => None,
        FusionChoice::Rrf => Some(Fusion::ReciprocalRank { k: RRF_K }),
        FusionChoice::Weighted => options
            .vector_weight()
            .map(|weight| Fusion::NormalizedScore {
                vector_weight: weight,
            }),
    }
}

/// Applies a declared fixed vector weight to an index read back from a segment.
///
/// A segment stores the ranking settings of the build that wrote it, so a
/// loaded index answers with the shipped adaptive weight whatever the table
/// declares. The weight is a query time setting that nothing in the stored
/// index depends on, so it is set on every index this table searches.
///
/// @param index - the index a search is about to use
/// @param options - the table's declaration
fn apply_ranking(index: &mut Index, options: &Options) {
    if let Some(fixed) = fixed_fusion(options) {
        index.set_fusion(fixed);
        index.set_adaptive_fusion(false, AdaptiveWeights::default());
    }
}

/// Maps the table's own metric spelling onto the one the retrieval engine
/// stores and builds its graph under.
///
/// Two enums rather than one: `inillucent-core` is a lower layer than this
/// crate and may not depend on it (`docs/invariants/layering.toml`), so it
/// cannot share `options::Metric` directly - the same reason `SavedFusion`
/// exists in `inillucent_core::persist` instead of serializing `Fusion` by
/// derive.
/// @param metric - the table's own declaration
fn core_metric(metric: Metric) -> inillucent_core::distance::Metric {
    match metric {
        Metric::Cosine => inillucent_core::distance::Metric::Cosine,
        Metric::L2 => inillucent_core::distance::Metric::L2,
    }
}

/// Refuses a generation whose graph was built under a different metric from
/// the one the table now declares.
///
/// **A generation is bytes on disk; a declaration is `%_config`.** The two
/// are written together and read together on every ordinary path, so they
/// cannot disagree by themselves - but nothing stops a `%_gen` row from one
/// table's history sitting under a declaration that no longer matches it
/// (the shadow tables are ordinary rows, and `store.rs`'s own invariant is
/// that they are exactly that). Answering with a graph built for a different
/// distance than the one just declared is a wrong order that looks like a
/// working index; refusing it, naming both metrics, is what
/// `inillucent_core::persist`'s own version check does for a format that
/// changed shape, and this is the same rule for a generation that changed
/// meaning instead.
/// @param index - the generation just read from disk
/// @param options - the table's current declaration
pub fn check_generation_metric(index: &Index, options: &Options) -> DbResult<()> {
    let stored = index.config().metric;
    let declared = core_metric(options.metric);
    if stored != declared {
        return Err(failure(format!(
            "inillucent_search: this generation was built under metric {stored:?}, \
             the table now declares {declared:?} - rebuild the index"
        )));
    }
    Ok(())
}

/// Builds a chunk from one stored row.
///
/// The content is every text column joined by newlines. When there is more than
/// one text column the first is *also* the heading path, so a chunk's text
/// begins with its heading - which is the shape the existing corpus has and the
/// shape the heading boost and proximity weighting were measured against.
///
/// **A facet column's value becomes an attribute rather than text**
/// (task-2067). That is what lets a search constrain it *inside* the scan: an
/// attribute is resolved against the store's own dictionary once per query and
/// then tested per chunk by `CompiledFilter::passes`, which the BM25 posting
/// loop and the graph walk both call before a candidate is admitted. Putting
/// the value in the indexed text instead would make it searchable and not
/// filterable, which is the opposite of what a facet is for - and it would
/// change the corpus statistics of the text beside it.
///
/// A one-column table has text and no heading, and that distinction matters
/// rather than being tidiness: the legacy migration declares exactly one column
/// and puts the source chunk's text in it verbatim, so that the terms and the
/// corpus statistics of the migrated index are the ones the source index had.
/// Fabricating a heading equal to the whole content would leave the two indexes
/// scoring differently the moment anybody turned the heading boost on.
///
/// Each row is its own document. That is a real consequence and it is stated
/// where it can be read: the per-document cap in the fusion never binds on a
/// `inillucent_search` table, because no two rows share a document. An application
/// that wants documents made of several chunks models them in SQL - a document
/// table and a join - which is what a relational engine is for.
/// @param id - the rowid, which is the document identity
/// @param row - the stored row
/// @param options - the table's declaration, which says which columns are facets
pub fn chunk_of(id: i64, row: &Row, options: &Options) -> ChunkInput {
    // Borrowed rather than cloned. This runs once per row of every build and
    // every fold, and a corpus is hundreds of thousands of rows: collecting the
    // columns as owned strings would allocate a copy of the whole corpus on the
    // way to joining it, which is what the join was already going to do once.
    let text: Vec<&str> = row
        .columns
        .iter()
        .enumerate()
        .filter(|(position, _)| !options.is_facet(*position))
        .map(|(_, value)| value.as_str())
        .collect();
    let heading = match text.len() > 1 {
        true => text.first().copied().unwrap_or_default().to_string(),
        false => String::new(),
    };
    let attributes: Vec<(String, Vec<String>)> = options
        .facets
        .iter()
        .filter_map(|position| {
            let name = options.facet_name(*position)?;
            let value = row.columns.get(*position)?.clone();
            Some((name, vec![value]))
        })
        .collect();
    ChunkInput {
        source: SOURCE.to_string(),
        external_doc_id: id.to_string(),
        chunk_index: 0,
        heading_path: if heading.is_empty() {
            Vec::new()
        } else {
            vec![heading.clone()]
        },
        content: text.join("\n"),
        title: heading,
        url: String::new(),
        space_key: None,
        author: None,
        author_id: None,
        updated_at: None,
        external_chunk_id: Some(id.to_string()),
        labels: Vec::new(),
        attributes,
        flags: Vec::new(),
        deleted: false,
    }
}

/// Returns the embedding one row contributes.
///
/// A row with no vector in a table that has a vector branch is a legitimate
/// state - a document whose embedding has not been computed yet - and it gets
/// the zero vector, which is orthogonal to nothing and therefore never a near
/// neighbour of anything. That is the honest answer: it is in the corpus
/// lexically and invisible to the vector branch until it is embedded.
///
/// **Every other width is a refusal (task-1932, M4).** This used to zero-pad a
/// short vector and truncate a long one, so a three wide row in an eight wide
/// index became a vector with five zeros on the end - which has a distance to
/// every query, is never obviously wrong, and is not the row's embedding. The
/// engine already had one answer to a width mismatch: `store::vector_of`
/// refuses it. Two answers to one question is worse than either, because the
/// one that refuses is the one a caller has written code for.
///
/// @param row - the row being folded into a segment
/// @param dims - the index's width
pub fn embedding_of(row: &Row, dims: usize) -> DbResult<Vec<f32>> {
    let width = dims.max(1);
    if row.vector.is_empty() {
        return Ok(vec![0.0f32; width]);
    }
    if row.vector.len() != width {
        return Err(failure(format!(
            "inillucent_search: this index has {width} dimensions, and a row's vector has {}",
            row.vector.len()
        )));
    }
    Ok(row.vector.clone())
}

#[cfg(test)]
mod width_tests {
    use super::*;

    /// A row whose vector is the wrong width is refused rather than reshaped.
    ///
    /// **There were two answers to one question (task-1932, M4).**
    /// `store::vector_of` refuses a width mismatch and this zero-padded a short
    /// vector and truncated a long one - so a three wide row in an eight wide
    /// index became a vector with five zeros on the end, which has a distance
    /// to every query and is not the row's embedding. Two answers is worse than
    /// either, because the one that refuses is the one a caller wrote code for.
    #[test]
    fn a_row_whose_vector_is_the_wrong_width_is_refused() {
        let narrow = Row {
            columns: vec!["body".to_string()],
            vector: vec![1.0, 2.0, 3.0],
        };
        let refused = embedding_of(&narrow, 8)
            .expect_err("a three wide vector in an eight wide index is refused");
        let said = refused
            .detail()
            .map(str::to_string)
            .unwrap_or_else(|| refused.message().to_string());
        assert!(
            said.contains('8') && said.contains('3'),
            "the refusal names neither width: {said}"
        );

        // A long one is refused the same way, where it used to be truncated.
        let wide = Row {
            columns: vec!["body".to_string()],
            vector: vec![1.0; 16],
        };
        assert!(
            embedding_of(&wide, 8).is_err(),
            "a sixteen wide vector in an eight wide index was truncated"
        );

        // The right width passes through unchanged.
        let exact = Row {
            columns: vec!["body".to_string()],
            vector: vec![0.5; 8],
        };
        assert_eq!(
            embedding_of(&exact, 8).expect("the right width passes"),
            vec![0.5f32; 8]
        );

        // And a row with no vector at all is still the zero vector, which is
        // the legitimate state this function was written for: a document whose
        // embedding has not been computed yet.
        let empty = Row {
            columns: vec!["body".to_string()],
            vector: Vec::new(),
        };
        assert_eq!(
            embedding_of(&empty, 8).expect("an empty vector is the zero vector"),
            vec![0.0f32; 8]
        );
    }
}

/// Builds a whole index from every row a search table holds, in one pass.
///
/// This is what the `compact` and `rebuild` commands both do. It is deliberately
/// not the incremental path: a graph grown one insert at a time is not the graph
/// a single-pass build produces, and only the single-pass build drops the chunks
/// an update tombstoned.
///
/// It used to say here that the single-pass graph is also better connected, and
/// a recall gate run against it did not find that. At 40,000 documents and 64 dimensions the
/// folded graph answered 0.704 recall at ten against the rebuilt graph's 0.637.
/// One reading over twenty-four probes is not a reversal of the claim, but it is
/// enough to stop the claim being made: what a rebuild is known to buy is the
/// tombstoned chunks going away.
/// @param context - the module's reach into the database
/// @param store - the shadow tables
/// @param options - the table's declaration
pub fn build_from_rows(
    context: &mut Context<'_>,
    store: &Store,
    options: &Options,
) -> DbResult<(Index, usize)> {
    let dims = options.dims.max(1);
    let mut chunks: Vec<ChunkInput> = Vec::new();
    let mut vectors: Vec<Vec<f32>> = Vec::new();
    store.scan_rows(context, |id, row| {
        chunks.push(chunk_of(id, &row, options));
        vectors.push(embedding_of(&row, dims)?);
        Ok(true)
    })?;
    let rows = chunks.len();
    let mut index = Index::new(configuration(options));
    if !chunks.is_empty() {
        index
            .add(chunks, &vectors)
            .map_err(|why| failure(why.to_string()))?;
    }
    index.commit();
    Ok((index, rows))
}

/// Builds a brand new segment from a bounded batch of deltas, touching no
/// existing segment at all.
///
/// **This is the automatic path, and never reading or rewriting anything
/// already published is the point.** The batch is collapsed to its last
/// operation per row - a row put and then deleted in the same batch leaves no
/// chunk to build and one tombstoned id to record - and only the rows left
/// with a `Put` become chunks in the new segment; a bare `Delete` becomes an
/// entry in [`SegmentMeta::tombstoned`] instead, because there is no content
/// to insert and a fold still has to be told the row is gone. The graph work
/// this pays is a build over the batch's own rows, the same one insert per
/// row that always happens for a corpus this size - it is bounded because the
/// batch is bounded, not because inserting is somehow cheaper here than it is
/// anywhere else.
///
/// The delta log's length is what bounds the batch. `compact = N` fixes it at
/// `N`; the default rule, `max(1024, rows / 8)`, grows with the corpus, so
/// that a table is not flushing a handful of rows every commit once it is
/// large - see `Options::compact_threshold`.
/// @param context - the module's reach into the database
/// @param store - the shadow tables
/// @param options - the table's declaration
/// @param pending - the delta entries this flush is publishing
pub fn build_segment_from_batch(
    context: &mut Context<'_>,
    store: &Store,
    options: &Options,
    pending: &[Delta],
) -> DbResult<(Index, usize, Vec<i64>)> {
    let dims = options.dims.max(1);
    let mut latest: Vec<(i64, Op)> = Vec::new();
    for entry in pending {
        match latest.iter_mut().find(|(id, _)| *id == entry.id) {
            Some(slot) => slot.1 = entry.op,
            None => latest.push((entry.id, entry.op)),
        }
    }
    let mut chunks: Vec<ChunkInput> = Vec::new();
    let mut vectors: Vec<Vec<f32>> = Vec::new();
    let mut tombstoned: Vec<i64> = Vec::new();
    for (id, op) in latest {
        match op {
            Op::Delete => tombstoned.push(id),
            Op::Put => match store.read_row(context, id)? {
                Some(row) => {
                    chunks.push(chunk_of(id, &row, options));
                    vectors.push(embedding_of(&row, dims)?);
                }
                // The log says the row was written and it is not there - the
                // row store is authoritative, so this is read as a delete,
                // exactly as the live-sync `apply` below already does.
                None => tombstoned.push(id),
            },
        }
    }
    let inserted = chunks.len();
    let mut index = Index::new(configuration(options));
    if !chunks.is_empty() {
        index
            .add(chunks, &vectors)
            .map_err(|why| failure(why.to_string()))?;
    }
    index.commit();
    tombstoned.sort_unstable();
    Ok((index, inserted, tombstoned))
}

/// What one call to [`fold_segment_recording`] actually applied, recorded
/// rather than only counted - the chunks and vectors it replaced or added,
/// and the bare ids it tombstoned, in the order it applied them.
///
/// This is what a merge checkpoint hands to
/// `inillucent_core::persist::write_segment_delta`: the batch a checkpoint
/// folds is bounded by the merge budget, so recording it rather than the
/// accumulator's total content is what keeps a checkpoint's own write
/// bounded the same way - see `persist.rs`'s segment delta section.
#[derive(Default)]
pub struct RecordedBatch {
    /// Every chunk this fold replaced or added, paired with its vector, in
    /// fold order.
    pub puts: Vec<(ChunkInput, Vec<f32>)>,
    /// Every `(source, external id)` pair this fold bare-tombstoned.
    pub tombstoned: Vec<(String, String)>,
}

/// Folds one already-loaded segment onto an accumulator, recording exactly
/// what was applied: its still-live documents by `replace_document`, then its
/// own bare-deleted ids by `tombstone` - the same two steps a query's own
/// fold (`refresh`, below) already applies to every segment it walks, now
/// shared by a merge checkpoint as well rather than only by a read.
///
/// **Fixes a defect this ticket's tests found.** Before this, folding a
/// segment applied only the `replace_document` half and never consulted
/// [`SegmentMeta::tombstoned`] at all - so a row deleted with no live chunk of
/// its own in the segment that recorded the delete (the shape every bare
/// delete takes; see `build_segment_from_batch`) came back alive after a
/// merge, whenever an older segment folded into the same accumulator still
/// held a live chunk for that id. `a_row_deleted_while_a_merge_is_in_flight_stays_deleted`
/// in `inillucent-compat/tests/engine/segment_merge_bound.rs` pins this: it fails
/// without the `tombstone` loop below, since that is the only line that ever
/// removes a chunk an *earlier* fold step added.
///
/// Only a segment's own still-live documents are read back for the first
/// half - a segment that has itself already absorbed an internal tombstone
/// (an earlier merge folding an older row together with a newer edit of it)
/// answers for the edit and not the row it replaced, so replaying the dead
/// half back in would resurrect it.
/// @param accumulator - the index being built up
/// @param segment - the segment being folded in
/// @param tombstoned - the ids that segment's own metadata says are deleted
pub fn fold_segment_recording(
    accumulator: &mut Index,
    segment: &Index,
    tombstoned: &[i64],
) -> DbResult<(usize, RecordedBatch)> {
    // Every segment of one table is built under the same `configuration`, so
    // a chunk's vector is already the accumulator's own width - there is
    // nothing here to pad or reconcile, only to copy across.
    //
    // **Every older version is tombstoned first and the segment goes in with
    // one append**, which is the same as replacing one document at a time,
    // because a segment holds one live document per id. One append is one
    // graph insert batch on every core the table builds with; one document at
    // a time was one insert at a time on one core, which made a merge step of
    // 8,000 chunks take 13 to 21 s and a first search over a table with many
    // segments take minutes.
    let mut recorded = RecordedBatch::default();
    let documents = live_documents_of(segment);
    let mut chunks = Vec::with_capacity(documents.len());
    let mut vectors = Vec::with_capacity(documents.len());
    for (id, chunk, vector) in documents {
        accumulator.tombstone(SOURCE, &id.to_string());
        recorded.puts.push((chunk.clone(), vector.clone()));
        chunks.push(chunk);
        vectors.push(vector);
    }
    let inserted = match chunks.is_empty() {
        true => 0,
        false => {
            accumulator
                .append(chunks, &vectors)
                .map_err(|why| failure(why.to_string()))?
                .chunks_added
        }
    };
    for id in tombstoned {
        accumulator.tombstone(SOURCE, &id.to_string());
        recorded
            .tombstoned
            .push((SOURCE.to_string(), id.to_string()));
    }
    Ok((inserted, recorded))
}

/// Folds one already-loaded segment onto an accumulator, the same as
/// [`fold_segment_recording`] without keeping what it applied - what a
/// caller that only wants the merged index (a query's own fold, `refresh`
/// below) reaches for instead.
/// @param accumulator - the index being built up
/// @param segment - the segment being folded in
/// @param tombstoned - the ids that segment's own metadata says are deleted
pub fn fold_segment(
    accumulator: &mut Index,
    segment: &Index,
    tombstoned: &[i64],
) -> DbResult<usize> {
    let (inserted, _recorded) = fold_segment_recording(accumulator, segment, tombstoned)?;
    Ok(inserted)
}

/// Returns every id dead once a merge's accumulator is finished: everything
/// any original input held live or bare-deleted, minus what the finished
/// accumulator still holds live.
///
/// Computing this from the originals, rather than threading a running touched
/// set through every checkpoint, is what lets a merge resume across several
/// commits without persisting anything beyond `MergeState` itself. None of the
/// originals is touched or removed until the caller
/// (`SearchTable::finish_merge`) swaps the finished segment into the manifest
/// in their place, so each one's ids can still be read then.
///
/// **What `finish_merge` needs, and all it needs.** The tombstone list is a set
/// of ids, so an input only has to say which ids it held live. Loading an
/// input to answer that read its graph, its lexical index and every vector,
/// and the commit that finished a level one merge of four inputs of 8,000
/// chunks reloaded all four and the accumulator as well. On the 2.0.7 report's
/// script, the slowest commits, which are the ones that finish such a merge,
/// went from 7.5, 7.4 and 6.2 s to 6.3, 6.2 and 4.8 s once they stopped. The
/// caller now passes the ids of the
/// inputs it folded in the same commit, which it read while folding them, and
/// loads only the inputs an earlier commit folded.
/// @param base - the finished merge's accumulator
/// @param touched - each input's metadata, with the ids it held live
pub fn dead_after_merge(base: &Index, touched: &[(SegmentMeta, Vec<i64>)]) -> Vec<i64> {
    let mut all: BTreeSet<i64> = BTreeSet::new();
    for (meta, ids) in touched {
        all.extend(ids.iter().copied());
        all.extend(meta.tombstoned.iter().copied());
    }
    let live: BTreeSet<i64> = live_ids_of(base).into_iter().collect();
    let mut tombstoned: Vec<i64> = all.difference(&live).copied().collect();
    tombstoned.sort_unstable();
    tombstoned
}

/// Returns the ids of a segment's live documents, and nothing else about them.
///
/// The same selection [`live_documents_of`] makes, without copying each
/// chunk's text and vector: a caller that only needs to know which ids a
/// segment holds paid for 3 KiB of vector per row at 768 numbers.
/// @param index - the segment to read
pub fn live_ids_of(index: &Index) -> Vec<i64> {
    let store = index.store();
    let mut out = Vec::with_capacity(store.n_chunks());
    for chunk in 0..store.n_chunks() {
        let chunk = chunk as u32;
        let Some(record) = store.chunks.get(chunk as usize) else {
            continue;
        };
        let Some(document) = store.documents.get(record.doc as usize) else {
            continue;
        };
        if document.deleted {
            continue;
        }
        if let Ok(id) = store.chunk_external_id(chunk).parse::<i64>() {
            out.push(id);
        }
    }
    out
}

/// Returns a segment's own live documents, ready to be replayed onto another
/// index or reported as a merge's touched set.
///
/// Reads the segment's own store and vector set rather than the rows it was
/// built from - a segment answers for itself once it exists, and rebuilding
/// its content from `%_content` would cost the corpus this function exists to
/// avoid touching.
/// @param index - the segment to read
fn live_documents_of(index: &Index) -> Vec<(i64, ChunkInput, Vec<f32>)> {
    let store = index.store();
    let vectors = index.vectors();
    let mut out = Vec::with_capacity(store.n_chunks());
    for chunk in 0..store.n_chunks() {
        let chunk = chunk as u32;
        let Some(record) = store.chunks.get(chunk as usize) else {
            continue;
        };
        let Some(document) = store.documents.get(record.doc as usize) else {
            continue;
        };
        if document.deleted {
            continue;
        }
        let Ok(id) = store.chunk_external_id(chunk).parse::<i64>() else {
            continue;
        };
        let heading_path: Vec<String> = store
            .heading_path(chunk)
            .into_iter()
            .map(str::to_string)
            .collect();
        let title = heading_path.first().cloned().unwrap_or_default();
        out.push((
            id,
            ChunkInput {
                source: SOURCE.to_string(),
                external_doc_id: id.to_string(),
                chunk_index: 0,
                heading_path,
                content: store.content(chunk).to_string(),
                title,
                url: String::new(),
                space_key: None,
                author: None,
                author_id: None,
                updated_at: None,
                external_chunk_id: Some(id.to_string()),
                labels: Vec::new(),
                attributes: attributes_of_document(store, record.doc),
                flags: Vec::new(),
                deleted: false,
            },
            vectors.copy_of(chunk),
        ));
    }
    out
}

/// Returns one document's facet values, grouped by facet name, as a
/// [`ChunkInput`] carries them.
///
/// **Why a merge needs this.** [`live_documents_of`] rebuilds each row from the
/// segment's store so it can be replayed onto the merge's accumulator. It used
/// to leave the attributes empty, so every row that went through a merge lost
/// its facet values. A search with `src = 'slack'` hands the facet to the
/// retrieval engine as an attribute filter, and a row with no attributes fails
/// it, so after enough small commits the search silently missed most of the
/// rows they wrote. `src IN ('slack')` still found them because the module does
/// not claim `IN`, and the engine checks the stored column itself.
/// `a_facet_equality_finds_rows_merged_from_small_commits` in
/// `inillucent-compat/tests/engine/search_facet_compare.rs` fails without it.
/// @param store - the segment's store
/// @param doc - the document's ordinal in that store
fn attributes_of_document(
    store: &inillucent_core::store::Store,
    doc: u32,
) -> Vec<(String, Vec<String>)> {
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
    for (name, value) in store.attributes_of(doc) {
        let Some(name_text) = store.attribute_names.value(*name) else {
            continue;
        };
        let Some(value_text) = store
            .attribute_values
            .get(*name as usize)
            .and_then(|values| values.value(*value))
        else {
            continue;
        };
        match grouped
            .iter_mut()
            .find(|(existing, _)| existing == name_text)
        {
            Some((_, values)) => values.push(value_text.to_string()),
            None => grouped.push((name_text.to_string(), vec![value_text.to_string()])),
        }
    }
    grouped
}

/// Loads one segment's bytes back into a queryable index, refusing a segment
/// delta chain (see `inillucent_core::persist`'s segment delta section) that
/// has not been sealed.
///
/// **This is the strict reader**, used by everything except a merge resuming
/// its own checkpoint: a query's fold, `finish_merge` publishing a merge's
/// result, `integrity-check`. A chain missing its seal is exactly the
/// half-written segment this format exists to make impossible to mistake for
/// a whole one - see [`load_segment_resumable`] for the one caller allowed
/// to see it anyway.
/// @param context - the module's reach into the database
/// @param store - the shadow tables
/// @param options - the table's declaration, to validate the metric
/// @param id - the segment's `%_gen` key
pub fn load_segment(
    context: &mut Context<'_>,
    store: &Store,
    options: &Options,
    id: i64,
) -> DbResult<Index> {
    let mut index = load_segment_by_id(context, store, options, id, true)?;
    check_generation_metric(&index, options)?;
    restore_dropped_facets(context, store, options, &mut index)?;
    Ok(index)
}

/// Loads one segment's bytes back into a queryable index without requiring
/// it to be sealed - the one caller allowed to see a segment delta chain
/// still in progress, because it is the very merge extending it.
///
/// `crate::module::SearchTable::continue_merge` is the only caller: it
/// re-reads its own accumulator to keep folding, and that accumulator's
/// newest link has no seal until the merge folding it is actually finished.
/// Every base link further back is read the same tolerant way regardless of
/// which entry point is used, because a base link never carries a seal of
/// its own - only the tip a caller asked for does, so [`load_segment`]'s
/// strictness and this function's leniency differ only in what they demand
/// of the *id given*, never of what that id's own base chain holds.
/// @param context - the module's reach into the database
/// @param store - the shadow tables
/// @param options - the table's declaration, to validate the metric
/// @param id - the segment's `%_gen` key, possibly still being extended
pub fn load_segment_resumable(
    context: &mut Context<'_>,
    store: &Store,
    options: &Options,
    id: i64,
) -> DbResult<Index> {
    let mut index = load_segment_by_id(context, store, options, id, false)?;
    check_generation_metric(&index, options)?;
    restore_dropped_facets(context, store, options, &mut index)?;
    Ok(index)
}

/// Puts back the facet values a merge in 2.0.3 and earlier wrote without.
///
/// Those merges rebuilt every row with no attributes (see
/// [`attributes_of_document`]), and the segments they wrote are still in
/// files made by those releases. Every row of a table with facets gets one
/// attribute per facet when it is built, even an empty one, so a live document
/// with none was damaged by such a merge. Its values are read back from the
/// row itself, which is authoritative. A table without that damage pays one
/// pass over its documents per load and reads nothing. The repair is in memory
/// only, and the next merge or compaction writes the repaired segment.
/// @param context - the module's reach into the database
/// @param store - the shadow tables, to read the rows from
/// @param options - the table's declaration, which says which columns are facets
/// @param index - the segment just loaded
fn restore_dropped_facets(
    context: &mut Context<'_>,
    store: &Store,
    options: &Options,
    index: &mut Index,
) -> DbResult<()> {
    if options.facets.is_empty() {
        return Ok(());
    }
    let mut damaged: Vec<(u32, i64)> = Vec::new();
    {
        let segment = index.store();
        for chunk in 0..segment.n_chunks() {
            let chunk = chunk as u32;
            let Some(record) = segment.chunks.get(chunk as usize) else {
                continue;
            };
            let Some(document) = segment.documents.get(record.doc as usize) else {
                continue;
            };
            if document.deleted || document.attributes.start != document.attributes.end {
                continue;
            }
            if let Ok(id) = segment.chunk_external_id(chunk).parse::<i64>() {
                damaged.push((record.doc, id));
            }
        }
    }
    for (doc, id) in damaged {
        if let Some(row) = store.read_row(context, id)? {
            let attributes = chunk_of(id, &row, options).attributes;
            index.set_document_attributes(doc, &attributes);
        }
    }
    Ok(())
}

/// Loads one `%_gen` id's bytes, resolving a segment delta's base chain as
/// many times as it points back - each base link read the same tolerant way
/// regardless of `require_sealed`, which only ever governs `id` itself.
/// @param context - the module's reach into the database
/// @param store - the shadow tables
/// @param options - the table's declaration
/// @param id - the `%_gen` key to load
/// @param require_sealed - whether `id` itself must carry a seal
fn load_segment_by_id(
    context: &mut Context<'_>,
    store: &Store,
    options: &Options,
    id: i64,
    require_sealed: bool,
) -> DbResult<Index> {
    let bytes = store.read_generation(context, id)?.ok_or_else(|| {
        failure(format!(
            "inillucent_search: segment {id} is named but not stored"
        ))
    })?;
    load_segment_bytes(context, store, options, id, &bytes, require_sealed)
}

/// Turns one `%_gen` row's bytes into a queryable index, dispatching on
/// whether they are an ordinary stream or a segment delta - see
/// `inillucent_core::persist::is_segment_delta`.
/// @param context - the module's reach into the database
/// @param store - the shadow tables
/// @param options - the table's declaration
/// @param id - the id these bytes were read from, for error messages
/// @param bytes - the bytes themselves
/// @param require_sealed - whether these specific bytes must carry a seal;
///   never propagated to a base link, which is read the same way regardless
fn load_segment_bytes(
    context: &mut Context<'_>,
    store: &Store,
    options: &Options,
    id: i64,
    bytes: &[u8],
    require_sealed: bool,
) -> DbResult<Index> {
    if !inillucent_core::persist::is_segment_delta(bytes) {
        return inillucent_core::persist::read_index_bytes(bytes).map_err(|error| {
            failure(format!(
                "inillucent_search: unreadable segment {id}: {error}"
            ))
        });
    }
    let parsed = inillucent_core::persist::parse_segment_delta(bytes).map_err(|error| {
        failure(format!(
            "inillucent_search: unreadable segment {id}: {error}"
        ))
    })?;
    if require_sealed && parsed.sealed.is_none() {
        return Err(failure(format!(
            "inillucent_search: segment {id} has not finished merging and cannot be read as a complete segment"
        )));
    }
    let base = match parsed.base {
        Some(base_id) => load_segment_by_id(context, store, options, base_id, false)?,
        None => {
            let mut index = Index::new(configuration(options));
            index.commit();
            index
        }
    };
    let index = inillucent_core::persist::apply_segment_delta(base, &parsed)
        .map_err(|why| failure(why.to_string()))?;
    if let Some((chunks, documents)) = parsed.sealed {
        let actual_chunks = index.store().n_chunks() as u64;
        let actual_documents = index.store().n_documents() as u64;
        if actual_chunks != chunks || actual_documents != documents {
            return Err(failure(format!(
                "inillucent_search: segment {id} claims {chunks} chunks and {documents} documents, its \
                 replayed content holds {actual_chunks} and {actual_documents}"
            )));
        }
    }
    Ok(index)
}

/// Every `%_gen` id reachable from `tip` by following its segment delta base
/// pointer, tip first, stopping at the first id that is not itself a
/// segment delta (an ordinary, already-published segment) or that is
/// missing entirely.
///
/// A merge in progress protects only its own accumulator id in
/// [`crate::store::state::MERGE`] - `crate::module::SearchTable`'s
/// `drop-old-generations` used to assume that was the whole of what an
/// in-flight merge depends on, which was true before this ticket, when an
/// accumulator was always one self-contained blob. Now the newest link is a
/// delta whose base is an *earlier* checkpoint's own id, unreferenced by
/// anything else in `%_state` - so reclaiming generations by that older
/// assumption would delete the very bytes the in-flight merge's next
/// checkpoint needs to resume from. This is what lets the command protect
/// the whole chain instead of only its tip.
/// @param context - the module's reach into the database
/// @param store - the shadow tables
/// @param tip - the chain's newest id
pub fn chain_ids(context: &mut Context<'_>, store: &Store, tip: i64) -> DbResult<Vec<i64>> {
    let mut ids = Vec::new();
    let mut current = tip;
    loop {
        ids.push(current);
        let Some(bytes) = store.read_generation(context, current)? else {
            break;
        };
        if !inillucent_core::persist::is_segment_delta(&bytes) {
            break;
        }
        let parsed = inillucent_core::persist::parse_segment_delta(&bytes).map_err(|error| {
            failure(format!(
                "inillucent_search: unreadable segment {current}: {error}"
            ))
        })?;
        match parsed.base {
            Some(base) => current = base,
            None => break,
        }
    }
    Ok(ids)
}

/// Brings the cached index up to the snapshot the statement is reading.
///
/// Three outcomes, in order of cost: nothing changed and the cache stands; the
/// live segments are the ones the cache was built from and only the delta
/// tail has grown, so the remainder is applied; or the segments themselves
/// disagree with what the cache holds, and every live segment is folded in
/// again from scratch. The third is what a flush, a merge, or a rollback
/// produces, and it is the one that has to be right rather than fast.
fn refresh(
    held: &mut Option<Cached>,
    context: &mut Context<'_>,
    store: &Store,
    options: &Options,
) -> DbResult<()> {
    let live = live_segments(context, store)?;
    let covered = live
        .iter()
        .map(|segment| segment.covers_to)
        .max()
        .unwrap_or(0);
    let mut ids: Vec<i64> = live.iter().map(|segment| segment.id).collect();
    ids.sort_unstable();
    let visible = store.deltas_above(context, covered)?;
    if let Some(cached) = held.as_mut() {
        if cached.segments == ids
            && visible.len() >= cached.applied.len()
            && visible.starts_with(&cached.applied)
        {
            let outstanding: Vec<Delta> = visible
                .get(cached.applied.len()..)
                .unwrap_or_default()
                .to_vec();
            if outstanding.is_empty() {
                return Ok(());
            }
            apply(&mut cached.index, context, store, options, &outstanding)?;
            cached.rows = live_rows_of(&cached.index);
            cached.applied = visible;
            return Ok(());
        }
    }
    let mut fold_order = live;
    fold_order.sort_by_key(|segment| segment.covers_from);
    let mut index = Index::new(configuration(options));
    index.commit();
    let mut first = true;
    for segment in &fold_order {
        let loaded = load_segment(context, store, options, segment.id)?;
        if first {
            // The oldest segment becomes the accumulator directly - it was
            // already committed when it was built, so every later
            // `replace_document` call updates it incrementally rather than
            // forcing a second full graph build here.
            index = loaded;
            first = false;
            for id in &segment.tombstoned {
                index.tombstone(SOURCE, &id.to_string());
            }
        } else {
            fold_segment(&mut index, &loaded, &segment.tombstoned)?;
        }
    }
    apply(&mut index, context, store, options, &visible)?;
    apply_ranking(&mut index, options);
    let rows = live_rows_of(&index);
    *held = Some(Cached {
        segments: ids,
        applied: visible,
        rows,
        index,
    });
    Ok(())
}

/// Reads the live segment manifest, migrating a pre-segment file's one
/// generation into a manifest of one on the fly.
///
/// **This is the one place "no `%_state` row named `segments`" is read, and it
/// must never read as "zero segments".** A table written before task-1911 has
/// no manifest at all - it has exactly the one generation `state::GENERATION`
/// and `state::COVERED` already name, which is one live segment covering
/// every row the log had folded by the time it was last published. Reading
/// the absence as an empty manifest would make a query over that file answer
/// zero rows, which is the exact defect `docs/roadmap.md`'s "What task-1911
/// closed" already records once for this engine.
/// @param context - the module's reach into the database
/// @param store - the shadow tables
pub fn live_segments(context: &mut Context<'_>, store: &Store) -> DbResult<Vec<SegmentMeta>> {
    if let Some(segments) = store.read_segments(context)? {
        return Ok(segments);
    }
    let generation = store.state(context, state::GENERATION)?;
    let covered = store.state(context, state::COVERED)?;
    let chunks = store.state(context, state::CHUNKS)?;
    Ok(legacy_manifest(generation, covered, chunks)
        .into_iter()
        .collect())
}

/// Synthesises the one-segment manifest a pre-task-1911 file implies, from
/// the three counters that file always had.
///
/// Pulled out of [`live_segments`] as a pure function so the "must not read
/// as zero segments" claim can be checked directly, without a database
/// behind it: `generation == 0` is the only case with nothing to
/// synthesise - a table that has never published anything - and every other
/// value has to come back as exactly one segment naming that generation,
/// however small `covered` or `chunks` are.
/// @param generation - `state::GENERATION`, as a legacy file left it
/// @param covered - `state::COVERED`, as a legacy file left it
/// @param chunks - `state::CHUNKS`, as a legacy file left it
fn legacy_manifest(generation: i64, covered: i64, chunks: i64) -> Option<SegmentMeta> {
    if generation == 0 {
        return None;
    }
    Some(SegmentMeta {
        id: generation,
        level: i32::MAX,
        covers_from: 0,
        covers_to: covered,
        chunks,
        tombstoned: Vec::new(),
    })
}

/// Applies a run of delta entries to a loaded index.
///
/// A put and an update are the same thing, because an append only index
/// tombstones whatever was there and appends the new version.
///
/// **Every changed row is tombstoned first and every new version goes in with
/// one append.** This applied one row at a time, and one row at a time is one
/// graph insert at a time on one core: a connection that had searched the table
/// paid 3.26 s on its next search after another process deleted and inserted
/// 2,000 rows of 768 numbers, against 0.02 s for the search after. One append
/// of the whole batch goes into the graph on every core the table builds with
/// (`HnswParams::build_threads`), which is how a fresh segment is already built.
///
/// Only the last entry for a row matters, because a put reads the row as the
/// row store holds it now rather than as the entry wrote it: a row deleted and
/// written again is written once, and a row written and then deleted is not
/// written at all.
fn apply(
    index: &mut Index,
    context: &mut Context<'_>,
    store: &Store,
    options: &Options,
    entries: &[Delta],
) -> DbResult<usize> {
    let dims = options.dims.max(1);
    let mut last: std::collections::HashMap<i64, Op> =
        std::collections::HashMap::with_capacity(entries.len());
    let mut order: Vec<i64> = Vec::with_capacity(entries.len());
    for entry in entries {
        if last.insert(entry.id, entry.op).is_none() {
            order.push(entry.id);
        }
    }
    let mut chunks = Vec::new();
    let mut vectors = Vec::new();
    for id in order {
        index.tombstone(SOURCE, &id.to_string());
        if last.get(&id) != Some(&Op::Put) {
            continue;
        }
        // The log says the row was written. If it is not there, the row store
        // is authoritative and the row stays deleted: that can only happen if
        // the two were written by different transactions, which the module
        // never does, or if something outside the module edited a shadow table.
        let Some(row) = store.read_row(context, id)? else {
            continue;
        };
        chunks.push(chunk_of(id, &row, options));
        vectors.push(embedding_of(&row, dims)?);
    }
    if chunks.is_empty() {
        return Ok(0);
    }
    let stats = index
        .append(chunks, &vectors)
        .map_err(|why| failure(why.to_string()))?;
    Ok(stats.chunks_added)
}

/// Returns how many live chunks an index holds.
fn live_rows_of(index: &Index) -> usize {
    let store = index.store();
    let total = store.n_chunks();
    let deleted = (store.deleted_ratio() * total as f32).round() as usize;
    total.saturating_sub(deleted)
}

/// Runs one request against a loaded index.
///
/// **The facet constraints are compiled into the filter the scan runs under,
/// not applied to the answer** (task-2067). The distinction is the whole reason
/// the feature exists and it is measurable: `Bm25Index::top_k` rescores the
/// best `k * rescore_depth_factor` hits by the position of the query's terms
/// inside them, rescoring only ever lowers a score, and a hit outside that
/// window keeps its full score and competes against rescored ones. Which hits
/// are inside the window depends on which chunks were admitted, so removing
/// rows after the ranking gives a different order from never admitting them -
/// measured at nine of the top ten hits on a 400 row corpus. Filtering
/// afterwards is also what leaves an answer shorter than the `k` it asked for.
///
/// @param index - the loaded index
/// @param options - the traversal settings and the facet declaration
/// @param request - what was asked for
/// @returns the hits, or the index's refusal of a query it cannot compare
fn run(index: &Index, options: &Options, request: &Request) -> DbResult<Vec<Hit>> {
    let limit = request.limit.max(1);
    let filter = index.compile(&filter_of(options, request)?);
    let ef = traversal_width(options, request, limit);
    let text = request.text.clone().unwrap_or_default();
    let has_text = !text.trim().is_empty();
    let has_vector = !request.vector.is_empty();
    let branches = match (has_text, has_vector) {
        (true, true) => inillucent_core::index::Branches::Both,
        (true, false) => inillucent_core::index::Branches::Lexical,
        (false, true) => inillucent_core::index::Branches::Vector,
        (false, false) => return Ok(Vec::new()),
    };
    let (hits, _) = index
        .search_branches(&text, &request.vector, &filter, limit, ef, branches)
        .map_err(|why| failure(why.to_string()))?;
    let store = index.store();
    Ok(hits
        .into_iter()
        .filter_map(|hit| {
            let external = store.chunk_external_id(hit.chunk);
            let id = external.parse::<i64>().ok()?;
            Some(Hit {
                id,
                score: hit.score,
                confidence: hit.confidence,
                origin: hit.origin,
            })
        })
        .collect())
}

/// Returns the predicate one request's facet constraints express.
///
/// Each constraint becomes an `AttributeFilter` on the facet's own name, which
/// is the name [`chunk_of`] filed the row's value under. Several of them narrow
/// rather than widen, which is what `WHERE live = '1' AND region = 'eu'` reads
/// as in SQL and what `CompiledFilter` does with a list of attribute
/// constraints.
///
/// A constraint naming a column the declaration does not call a facet is a
/// refusal rather than a constraint that is quietly dropped. The access path
/// claims a facet constraint, which tells the engine not to evaluate it again,
/// so dropping one here would answer rows that do not satisfy the `WHERE`
/// clause.
///
/// @param options - the table's declaration
/// @param request - what was asked for
fn filter_of(options: &Options, request: &Request) -> DbResult<Filter> {
    let mut filter = Filter::default();
    for (position, value) in &request.facets {
        let Some(name) = options.facet_name(*position) else {
            return Err(failure(format!(
                "inillucent_search: column {position} is not a facet of this table, so a search \
                 cannot rank over it"
            )));
        };
        filter
            .attributes
            .push(AttributeFilter::any_of(&name, &[value.as_str()]));
    }
    Ok(filter)
}

/// Returns the traversal width one request asks for.
///
/// `None` means "the configured default"; an exact table has already been
/// configured never to traverse at all, so the number only matters to an
/// approximate one. Recall is a monotone control on breadth, not a guarantee:
/// the only value with a guarantee behind it is 1.0, which asks for every
/// comparison to be made and therefore for the exact answer.
fn traversal_width(options: &Options, request: &Request, limit: usize) -> Option<usize> {
    let recall = request.recall?;
    if options.mode == Mode::Exact {
        return None;
    }
    if recall >= 1.0 {
        return Some(usize::MAX);
    }
    let clamped = recall.clamp(0.0, 0.999);
    let oversample = (1.0 / (1.0 - clamped)).min(MAX_OVERSAMPLE);
    Some(((limit as f32 * oversample).ceil() as usize).max(limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options;

    fn declaration(source: &[&str]) -> Options {
        let arguments: Vec<Vec<u8>> = source.iter().map(|text| text.as_bytes().to_vec()).collect();
        options::parse(&arguments).expect("parsed")
    }

    /// An exact table is configured never to traverse the graph.
    #[test]
    fn an_exact_table_never_traverses() {
        let config = configuration(&declaration(&["body", "dims = 8"]));
        assert_eq!(config.hnsw.exhaustive_below, usize::MAX);
        assert_eq!(config.dims, 8);
    }

    /// An approximate table keeps the graph's own cost model.
    #[test]
    fn an_approximate_table_keeps_the_cost_model() {
        let config = configuration(&declaration(&["body", "dims = 8", "mode = approximate"]));
        assert!(config.hnsw.exhaustive_below < usize::MAX);
    }

    /// A lexical-only table still has one dimension, so every structure is
    /// indexed the same way.
    #[test]
    fn a_lexical_table_has_one_dimension() {
        let config = configuration(&declaration(&["body"]));
        assert_eq!(config.dims, 1);
    }

    /// A table with no `metric` argument builds the graph as cosine, which is
    /// the only metric a table with nothing declared could ever have meant.
    #[test]
    fn configuration_defaults_to_cosine() {
        let config = configuration(&declaration(&["body", "dims = 8"]));
        assert_eq!(config.metric, inillucent_core::distance::Metric::Cosine);
    }

    /// `metric = 'l2'` reaches `IndexConfig`, which is the field
    /// `VectorSet::push` reads to decide whether to normalize - the one line
    /// this whole feature turns on.
    #[test]
    fn configuration_carries_a_declared_l2_metric() {
        let config = configuration(&declaration(&["body", "dims = 8", "metric = l2"]));
        assert_eq!(config.metric, inillucent_core::distance::Metric::L2);
    }

    /// A generation built under the metric the table still declares is
    /// accepted.
    #[test]
    fn a_generation_matching_the_declared_metric_is_accepted() {
        let l2 = declaration(&["body", "dims = 4", "metric = l2"]);
        let mut index = Index::new(configuration(&l2));
        index.commit();
        assert!(check_generation_metric(&index, &l2).is_ok());
    }

    /// A generation built under one metric, opened by a table that now
    /// declares the other, is refused rather than silently searched as if it
    /// agreed - and the message names both, so a person reading it knows
    /// which one is wrong and what the fix is (rebuild).
    #[test]
    fn a_mismatched_generation_is_refused_naming_both_metrics() {
        let declared_l2 = declaration(&["body", "dims = 4", "metric = l2"]);
        // A generation actually built under cosine - as if an older
        // declaration built it, or a generation from a different table's
        // history ended up under this one's row.
        let cosine_config = configuration(&declaration(&["body", "dims = 4"]));
        let mut built_under_cosine = Index::new(cosine_config);
        built_under_cosine.commit();

        let error = check_generation_metric(&built_under_cosine, &declared_l2)
            .expect_err("a metric mismatch must be refused");
        let message = format!("{error:?}");
        assert!(
            message.contains("Cosine"),
            "should name the stored metric: {message}"
        );
        assert!(
            message.contains("L2"),
            "should name the declared metric: {message}"
        );
    }

    /// A chunk's text begins with its heading, which is what the ranking
    /// settings were measured against.
    #[test]
    fn a_chunk_begins_with_its_heading() {
        let row = Row {
            columns: vec!["Offer eligibility".to_string(), "who qualifies".to_string()],
            vector: Vec::new(),
        };
        let chunk = chunk_of(7, &row, &declaration(&["heading", "body"]));
        assert!(chunk.content.starts_with("Offer eligibility"));
        assert_eq!(chunk.heading_path, vec!["Offer eligibility".to_string()]);
        assert_eq!(chunk.external_chunk_id.as_deref(), Some("7"));
    }

    /// A facet column's value is filed as an attribute and is not in the text.
    ///
    /// Both halves matter. The value has to be an attribute or a query cannot
    /// constrain the ranking by it; it has to be out of the text or it would
    /// change the terms, the chunk length and therefore the score of the prose
    /// beside it.
    #[test]
    fn a_facet_column_is_filed_as_an_attribute_and_not_as_text() {
        let row = Row {
            columns: vec!["who qualifies".to_string(), "1".to_string()],
            vector: Vec::new(),
        };
        let chunk = chunk_of(7, &row, &declaration(&["body", "live facet"]));
        assert_eq!(chunk.content, "who qualifies");
        assert!(chunk.heading_path.is_empty(), "one text column, no heading");
        assert_eq!(
            chunk.attributes,
            vec![("live".to_string(), vec!["1".to_string()])]
        );
    }

    /// A facet constraint compiles into a predicate on the facet's own name.
    #[test]
    fn a_facet_constraint_becomes_an_attribute_predicate() {
        let options = declaration(&["body", "live facet"]);
        let request = Request {
            facets: vec![(1, "1".to_string())],
            ..Request::default()
        };
        let filter = filter_of(&options, &request).expect("compiled");
        assert_eq!(filter.attributes.len(), 1);
        assert_eq!(
            filter.attributes.first().map(|held| held.name.as_str()),
            Some("live")
        );
        assert!(
            !filter.is_empty(),
            "a facet constraint has to be a predicate"
        );
    }

    /// A constraint on a column the declaration does not call a facet refuses.
    ///
    /// It cannot be dropped: the access path claims the constraint, which tells
    /// the engine not to evaluate it, so a dropped one would answer rows the
    /// `WHERE` clause excluded.
    #[test]
    fn a_constraint_on_a_column_that_is_not_a_facet_refuses() {
        let options = declaration(&["body", "live facet"]);
        let request = Request {
            facets: vec![(0, "1".to_string())],
            ..Request::default()
        };
        let failed = filter_of(&options, &request).expect_err("refused");
        assert!(
            failed.message().contains("not a facet")
                || failed
                    .detail()
                    .is_some_and(|said| said.contains("not a facet")),
            "should say which column is not a facet: {}",
            failed.message()
        );
    }

    /// A row with no embedding joins the corpus lexically and is invisible to
    /// the vector branch rather than being refused.
    #[test]
    fn a_row_with_no_embedding_gets_the_zero_vector() {
        let row = Row {
            columns: vec!["text".to_string()],
            vector: Vec::new(),
        };
        assert_eq!(
            embedding_of(&row, 4).expect("an empty vector is the zero vector"),
            vec![0.0, 0.0, 0.0, 0.0]
        );
    }

    /// Recall is a monotone control on breadth, and one asks for the exact path.
    #[test]
    fn recall_widens_the_traversal() {
        let table = declaration(&["body", "dims = 4", "mode = approximate"]);
        let narrow = traversal_width(
            &table,
            &Request {
                recall: Some(0.5),
                ..Request::default()
            },
            10,
        );
        let wide = traversal_width(
            &table,
            &Request {
                recall: Some(0.9),
                ..Request::default()
            },
            10,
        );
        assert!(narrow < wide);
        assert_eq!(
            traversal_width(
                &table,
                &Request {
                    recall: Some(1.0),
                    ..Request::default()
                },
                10
            ),
            Some(usize::MAX)
        );
    }

    /// An exact table ignores recall, because it is already exact.
    #[test]
    fn an_exact_table_ignores_recall() {
        let table = declaration(&["body", "dims = 4"]);
        assert_eq!(
            traversal_width(
                &table,
                &Request {
                    recall: Some(0.1),
                    ..Request::default()
                },
                10
            ),
            None
        );
    }

    /// A table that has never published a generation has no segment to
    /// synthesise - there is nothing on disk for `live_segments` to name.
    #[test]
    fn a_table_that_never_published_has_no_legacy_segment() {
        assert_eq!(legacy_manifest(0, 0, 0), None);
    }

    /// A pre-task-1911 file - one generation, no manifest row - reads as
    /// exactly one live segment naming that generation, covering everything
    /// up to `covered`. This is the claim `docs/roadmap.md`'s "What
    /// task-1911 closed" already had to fix once for a different reason: a
    /// table with rows and a published generation must never be answered as
    /// if it held zero segments.
    ///
    /// **Fails without the change:** this is a new function, so there is
    /// nothing to revert it against; it exists because reverting
    /// `crates/inillucent-search` and asking `live_segments` this same
    /// question would return `Vec::new()` - the pre-task-1911 code has no
    /// concept of a segment at all, and every query over a real, non-empty
    /// legacy file would then search an empty index and answer zero rows.
    #[test]
    fn a_legacy_generation_reads_as_one_live_segment() {
        let segment = legacy_manifest(4, 812, 4_096).expect("a published table has one segment");
        assert_eq!(
            segment.id, 4,
            "the segment is named by the old generation counter"
        );
        assert_eq!(
            segment.covers_from, 0,
            "it covers everything from the start"
        );
        assert_eq!(segment.covers_to, 812);
        assert_eq!(segment.chunks, 4_096);
        assert!(segment.tombstoned.is_empty());
    }
}
