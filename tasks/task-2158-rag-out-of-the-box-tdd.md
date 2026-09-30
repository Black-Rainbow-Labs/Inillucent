# RAG out of the box: a reranker, a fusion option, bulk embedding, chunking, and truncation reports

## Summary

A retrieval study on 29 September 2026 measured which search techniques find the right text for a
language model. It ran on a private mailbox of 67,369 emails, with 119 graded questions. The
beginner explanation of the study, its terms and every number is `docs/rag-explained.md`. Read that
page first. This design turns its recommendations into features of inillucent, so a user gets the
winning setup without writing it themselves.

Six changes, in the order the study ranked them:

| # | Change | Why, from the study |
|---|---|---|
| 1 | A built in reranker: the SQL function `rerank(query, passage)`, and reranking inside an `inillucent_search` query | +0.10 to +0.13 nDCG@10 on every collection and every embedding model. The largest gain measured |
| 2 | A `fusion` table option on `inillucent_search`, with `'rrf'` as a value | RRF beat the adaptive weight by 0.035 on the mailbox. inillucent's own score card found the opposite, so the default does not change |
| 3 | Bulk embedding of a column, on the CPU or a graphics card: the command `inillucent embed` | 558,429 chunks took 22 minutes on the GPU. `embed()` in SQL, one row at a time on the CPU, would take about 12 hours |
| 4 | A chunking function, `chunk_text(text, size, overlap, heading)` | Chunks without their heading lost 0.043 to 0.111 nDCG@10 |
| 5 | A thread count setting and a device setting for the ONNX Runtime sessions | `embed()` has no thread setting today, so two processes embedding at once oversubscribe the CPU. Four threads was the fastest single process setting measured |
| 6 | Truncation reporting: `embed_tokens(text)` and a count in the bulk command | 19.5% of 6,000 character chunks were cut at the 1,900 token limit with no message |

What this design does not do:

- It does not change any default ranking. The adaptive weight stays the default fusion.
- It does not add query rewriting or HyDE. The study measured no gain from either.
- It does not add language model summaries. They helped only when no reranker ran.
- It does not change Nikaya. Nikaya is a separate repository and gets its own ticket.
- It does not cut a release. The next release carries it.

## Terms

| Term | Meaning |
|---|---|
| cross encoder | a model that reads a question and a passage together and returns one relevance score. A reranker is a cross encoder |
| bi encoder | a model that reads one text and returns a vector. `nomic-embed-text-v1.5` is a bi encoder |
| logit | the raw number a model outputs before it is squashed into a range. A reranker's logit becomes a score from 0 to 1 through the sigmoid function, `1 / (1 + e^-x)` |
| execution provider | the part of ONNX Runtime that runs a model on one kind of processor. `CPU` and `CUDA` are the two used here |
| session | a model loaded by ONNX Runtime and ready to run. `OnnxEmbedder` owns one |
| residency | when a loaded session is kept in memory and when it is dropped. `crates/inillucent-core/src/residency.rs` |
| candidate depth | how many rows of a first search the reranker scores |

## What exists today

A code map, so the implementer does not have to find it again. Paths are from the repository root.

**Embedding.**
- `crates/inillucent-core/src/embed_onnx.rs` holds `OnnxEmbedder`. `open_model` (about line 412) builds
  the ONNX Runtime session. `OnnxOptions` (about line 155) already has `intra_threads`, `device`
  (`Device::Cpu` or `Device::Cuda(id)`), `batch_size`, `max_batch_cells` and `max_tokens`. CUDA
  registration with `error_on_failure()` already works. `plan_batches` groups texts under a cell
  limit. `embed_prefixed` clamps each text to `max_tokens` (1,900) and counts truncation in atomics
  that `OnnxEmbedder::truncation()` returns as `TruncationFacts`. Nothing reads those counts outside
  the bench tool.
- `crates/inillucent-core/src/residency.rs` holds `ManagedEmbedder`, the lazy loader with the
  `resident`, `on-demand` and `idle:<time>` profiles, and a reaper thread.
- `crates/inillucent-core/src/model.rs` holds `ModelManifest` (`model.json`), with `nomic_v1_5()`.
  `Backend` and `Output` describe a bi encoder only.
- `crates/inillucent-core/src/install.rs` finds the install folder, the runtime and the model folder.
  `DEFAULT_MODEL` is `nomic-embed-text-v1.5`.
- `crates/inillucent-search/src/embed.rs` registers the SQL scalar `embed(TEXT)`. Its `build()`
  calls `OnnxOptions::for_model`, so the SQL path always runs on the CPU with ONNX Runtime's default
  thread count. One process wide `static EMBEDDER`.
- `crates/inillucent-search/src/embed_refusal.rs` holds the refusal messages, outside the feature
  gate.
- `crates/inillucent-sql/src/function.rs`, `NEEDS_A_COMPONENT`, lists functions that a build without
  the `embed` feature refuses with `unsupported` (exit code 3) instead of "no such function".
- `crates/inillucent-cli/src/setup.rs` is `inillucent setup-embeddings`. It pins every download by
  SHA-256 (`RUNTIMES`, `NOMIC_FILES`) and downloads through `inillucent_remote::http::download`.
  `--gpu` already installs the CUDA build of ONNX Runtime 1.22.0.

**Search.**
- `crates/inillucent-search/src/options.rs` parses `CREATE VIRTUAL TABLE ... USING
  inillucent_search(...)`. A new option needs four edits: a field on `Options`, an arm in `parse`, a
  row in `config_rows`, and a read in `from_config`. An unknown `%_config` key is ignored by an older
  reader.
- `crates/inillucent-search/src/module.rs` declares the hidden columns (`declaration_of`, about line
  139): the table name, `k`, `vector`, `recall`, `rank`, and the plan roles
  `ROLE_QUERY/LIMIT/VECTOR/RECALL/ROWID`. `column` (about line 1432) returns `rank`, `score`,
  `confidence` and `origin`.
- `crates/inillucent-search/src/merge.rs`: `run` (about line 1204) runs a search. `configuration`
  and `apply_ranking` (about lines 302 to 330) turn `vector_weight` into
  `Fusion::NormalizedScore` with adaptive fusion off.
- `crates/inillucent-core/src/rank.rs` already has `Fusion::ReciprocalRank { k }`, `RRF_K = 60.0`,
  and the fusion loop. `crates/inillucent-core/src/index.rs` `IndexConfig::default()` picks
  `Fusion::NormalizedScore { vector_weight: 0.35 }` with `adaptive_fusion: true`. Its comment records
  why: on the score card corpus min-max at 0.35 beat RRF. `persist.rs` `SavedFusion` already
  round trips `"rrf"`.

**Functions and table functions.**
- `crates/inillucent-ext/src/registry.rs`: `UserFunction`, `register_function`, `with_builtins`,
  `register_module`.
- `crates/inillucent-ext/src/vtab/series.rs` (`generate_series`) is the smallest complete table
  valued function. A table valued function is an eponymous virtual table module: visible output
  columns, hidden argument columns, `best_index`, and a cursor.

**Commands, capabilities, tests.**
- `crates/inillucent-cli/src/command/registry.rs` is the one command table. The command line and
  the MCP server are both generated from it. `command_parity` fails when they disagree.
- `drivers/inillucent-driver/src/capability.rs` `CAPABILITIES`, checked in both directions by
  `drivers/inillucent-driver/tests/capability.rs`.
- `tests/selection.toml` needs a row for every new test file. Model dependent tests use
  `features = ["inillucent-engine/embed"]` and `requires = ["embed"]`, and skip through
  `inillucent_base::testing::skipping` when the model is absent. See `rag_verify.rs` and
  `embed_direct_only.rs` for the two patterns.

**The study's own code**, for reference only: `tools/rag-lab/rerank.py` in the study repository is
the cross encoder the study measured. Its input and output handling is what item 1 reproduces in
Rust.

## 1. A built in reranker

### What the user writes

```sh
inillucent setup-embeddings reranker          # about 600 MB, once
```

As a function, for any query:

```sql
SELECT id, body, rerank(?1, body) AS relevance
FROM chunk
WHERE id IN (SELECT rowid FROM chunk_search WHERE chunk_search MATCH ?1 AND k = 60)
ORDER BY relevance DESC
LIMIT 10;
```

Inside a search table, which batches the work and is the path to recommend:

```sql
CREATE VIRTUAL TABLE chunk_search USING inillucent_search(title, body, dims = 768, rerank_depth = 60);

SELECT rowid, title, score(chunk_search) AS relevance
FROM chunk_search
WHERE chunk_search MATCH ?1
  AND vector = embed('search_query: ' || ?1)
  AND question = ?1
  AND k = 10
ORDER BY rank;
```

A cross encoder takes the question with no prefix. The `search_query: ` prefix belongs to
`nomic-embed-text-v1.5` only, so every example passes the plain question to `rerank()` and to
`question`.

### The model

Ship one model: `gte-reranker-modernbert-base` from `Alibaba-NLP/gte-reranker-modernbert-base`.

| Model | Parameters | File | nDCG@10 gain on the sample | 60 candidates on the GPU | 60 on the CPU |
|---|---|---|---|---|---|
| `gte-reranker-modernbert-base` | 149 million | `onnx/model.onnx`, 599 MB | +0.101 | 0.14 s | 10.6 s |
| `bge-reranker-v2-m3` | 568 million | `onnx/model.onnx` plus a 2.3 GB `model.onnx_data` | +0.106 | 0.23 s | 22.8 s |

The two scored the same (+0.004, interval -0.017 to +0.026). gte is a quarter of the size, is one
file, and is faster on both processors. bge is not shipped.

Pin a Hugging Face commit, not `main`: `resolve/<commit>/onnx/model.onnx`. Find the commit id with
the Hugging Face API (`https://huggingface.co/api/models/Alibaba-NLP/gte-reranker-modernbert-base`,
field `sha`). Pin the SHA-256 of every file: `onnx/model.onnx`, `tokenizer.json`, and whatever the
tokenizer needs (`tokenizer_config.json`, `special_tokens_map.json`, `config.json`). The study's
copy of `onnx/model.onnx` had SHA-256
`c6d3226502addbcd4d2cf273802957ebf8a2a6bf94037dcb9b1d95bfc01e5d93`, in
the study machine's `rerankers/gte-reranker-modernbert-base/` folder. The pinned file must match a
fresh download from the pinned commit. Read the model's license from the repository and record it
next to the pin; Apache 2.0 is expected.

It installs to `models/gte-reranker-modernbert-base/` in the install folder, with its own
`model.json`.

### The manifest

Extend `ModelManifest` in `crates/inillucent-core/src/model.rs` with the kind of model:

```json
{ "id": "gte-reranker-modernbert-base", "kind": "cross_encoder", "max_tokens": 1024, ... }
```

- `kind` is `"bi_encoder"` or `"cross_encoder"`. A manifest with no `kind` is a bi encoder, so every
  existing `model.json` and cache digest still reads the same. Check how the manifest digest is
  computed (`docs/embeddings.md`, "A manifest field would change the manifest digest"). Adding a
  field must not change the digest of an existing bi encoder manifest. If the digest covers every
  field, leave `kind` out of the digest when it is `bi_encoder`, and add a test that the digest of
  `nomic_v1_5()` is unchanged.
- `max_tokens` is 1,024 for the reranker, the value the study used. The model accepts 8,192, and a
  longer input costs time with the square of its length.
- Add `gte_reranker_modernbert_base()` beside `nomic_v1_5()`.

### The Rust type

New file `crates/inillucent-core/src/rerank_onnx.rs`, behind the existing `onnx` feature:

```rust
pub struct OnnxReranker { /* session: Mutex<Session>, tokenizer, manifest, counters */ }

impl OnnxReranker {
    /// Loads the cross encoder from a model folder.
    pub fn open_model(dir: &Path, options: &OnnxOptions) -> Result<Self, EmbedError>;
    /// One score from 0 to 1 for each passage, in input order. Higher means more relevant.
    pub fn score(&self, query: &str, passages: &[&str]) -> Result<Vec<f32>, EmbedError>;
}
```

How `score` works, matching `rerank.py`:

1. Tokenize each `(query, passage)` pair with the tokenizer's pair encoding. Turn the tokenizer's
   own padding off, as `disarm_tokenizer` does for the embedder.
2. Truncate only the passage, so the pair fits in `max_tokens`. The question is never cut. If the
   question alone is longer than `max_tokens`, cut the question and count it as truncated.
3. Group pairs with `plan_batches` under `max_batch_cells`.
4. Feed `input_ids` and `attention_mask`. Feed `token_type_ids` as zeros only when the session
   declares that input. ModernBERT does not. Read the input names from the session.
5. The output is a logit per pair, shape `[batch, 1]`. Return `1 / (1 + exp(-logit))`.
6. Count pairs seen and pairs truncated, as `OnnxEmbedder` does.

Share code with `embed_onnx.rs` where it is the same: session building (`open_model`'s runtime
loading, optimization level, intra threads, CUDA registration), `plan_batches`, and
`disarm_tokenizer`. Move the shared parts into functions both files call. Do not copy them.

Residency: generalize `ManagedEmbedder` so it can hold either kind of session, or add a
`ManagedReranker` that uses the same `Residency` profile and reaper. Whichever is smaller. The
profile is the same one `setup-embeddings --residency` records.

### The SQL function

`rerank(query TEXT, passage TEXT) -> REAL`, registered in `crates/inillucent-search/src/embed.rs`
(or a new `rerank.rs` next to it) the way `embed` is:

- deterministic, and `direct_only` through `UserFunction::external`;
- `NULL` in either argument returns `NULL`;
- a process wide `static RERANKER`, like `static EMBEDDER`;
- one pair per call. Say in the documentation that the search table path batches and is faster.
- when no reranker is installed: status `invalid_state`, exit code 1, with a message that names
  `inillucent setup-embeddings reranker`. Add it to `embed_refusal.rs`.
- add `rerank` to `NEEDS_A_COMPONENT` in `crates/inillucent-sql/src/function.rs`, so a build without
  the `embed` feature answers `unsupported` with exit code 3.

### Reranking inside `inillucent_search`

Two additions:

| Name | Kind | Meaning | Default |
|---|---|---|---|
| `rerank_depth = N` | table option | how many rows of the fused list the reranker scores. 1 to 1,000 | 60 |
| `question` | hidden column, used as `question = ?1` | the question in plain words. Naming it turns reranking on for that query | none |

The rule is simple: a query that names `question` is reranked, and a query that does not is not.
`rerank_depth` only says how deep.

When the query names `question`:

1. The search collects `max(k, rerank_depth)` rows with the fusion the table declares.
2. For each of the top `rerank_depth` rows, the passage is the table's text columns in declaration
   order, joined with a newline. Facet columns are not text and are left out. This is how the
   heading reaches the reranker: a table declared as `(title, body)` gives `title\nbody`.
3. `OnnxReranker::score` scores them in one call, so the batching applies.
4. Rows are ordered by the reranker score, highest first. Ties keep the fused order.
5. The first `k` rows are returned.
6. `score(t)` returns the reranker score, from 0 to 1. `rank` is `-score` as today.
   `confidence(t)` and `origin(t)` do not change: `confidence` stays the fused confidence the
   documentation already explains.

Refusals, each with a message that says what to do:

- `question` with neither `MATCH` nor `vector`: status `invalid_state`. The reranker needs
  candidates from a search.
- `question` in a build without the `embed` feature: status `unsupported`, exit code 3.
- `question` with no reranker installed: the `invalid_state` message from the SQL function.
- `rerank_depth` outside 1 to 1,000, or not an integer: refused at `CREATE`.

`rerank_depth` is stored in `%_config`. A table without the key reads it as 60. A reader from before
this change ignores the key, and ignores `question` too, because it has no such column: it refuses
the query with "no such column". That is acceptable. Note it in `CHANGELOG.md`.

Also accept `question` in the table function form. Today the arguments are query text, `k`,
vector, recall. Add `question` as the fifth.

### The command line and MCP

- `inillucent setup-embeddings reranker` installs the reranker. `all` keeps its meaning (runtime and
  embedding model) so nobody gets an unexpected 600 MB. `--status` reports the reranker too.
- `inillucent search` gets a `--rerank` flag. When set, the command passes the query text as
  `question`. The MCP tool `inillucent_search` gets the same parameter from the same table row.

### Capability row

Add a `rerank` row to `CAPABILITIES`. Its probe must work on a machine with no model: a `Refuses`
or `Registers` probe that checks the function is registered in an `embed` build, the way the table
checks other features that need a component. Read the neighbouring rows and pick the probe that the
two direction test accepts.

### Measurements to record

A number in the documentation needs a source. Record these on this machine, with the commit and
the date, in `docs/embeddings.md` (a new section "Reranking"):

1. **Time.** Median and 95th percentile of `OnnxReranker::score` over 20 and over 60 passages of
   about 900 characters, on the CPU with 4 and 8 threads and on `cuda:0`. Add a subcommand to
   `inillucent-bench` for it (`rerank-cost`) so the number can be measured again. Ask for a quiet
   window for the CPU rows (the task skill describes `QUIET REQUEST`).
2. **Agreement with the study.** Score the same 20 pairs with the Python `rerank.py` (the study's
   Python environment) and with `OnnxReranker`. The scores must agree
   to 1e-4 after the sigmoid. Record the largest difference.
3. **Quality on a public corpus.** On `examples/rag-agent` (3,696 chunks, 20 answerable questions),
   compare the documented hybrid search with and without `question`. Report "found in top 5" and
   mean reciprocal rank beside the existing table in `docs/vector-search.md`, "How the two rankings
   are combined". Whatever the result, print it. If reranking does not help on that corpus, say so.

## 2. A `fusion` table option

```sql
CREATE VIRTUAL TABLE chunk_search USING inillucent_search(title, body, dims = 768, fusion = 'rrf');
```

| Value | Meaning |
|---|---|
| `'adaptive'` | today's default: min-max scaling, vector weight 0.35, adjusted for each query |
| `'rrf'` | reciprocal rank fusion with k = 60: `Fusion::ReciprocalRank { k: RRF_K }`, adaptive off |
| `'weighted'` | min-max scaling with the fixed `vector_weight`. Requires `vector_weight` |

Rules:

- No `fusion` and no `vector_weight` means `'adaptive'`. The default does not change. Explain why in
  a comment beside the parse arm: the mailbox study favoured RRF by 0.035, and the score card corpus
  favoured adaptive by 0.065 on document identity. Two corpora disagree, and changing the default
  would change every existing user's results.
- `vector_weight` alone means `'weighted'`, as today.
- `fusion = 'rrf'` with `vector_weight` is refused at `CREATE`: RRF has no weight.
- `fusion = 'weighted'` without `vector_weight` is refused.
- Stored in `%_config` as `fusion`. A table with no `fusion` key reads as today.
- Implement in `merge::configuration` and `apply_ranking`, beside the `vector_weight` branch.
- `confidence(t)` is unchanged: it is already computed the theoretical way whatever fusion runs.
- The score of an RRF search is small, at most `2 / 61`, about 0.033. Say so in the documentation,
  so nobody sets a threshold on `score` expecting 0 to 1.

Tests: the RRF example from `docs/rag-explained.md` (lists A, B, C and C, D, A) gives A and C first
with score `1/61 + 1/63`. Build that case with a keyword query and a vector that reproduce those two
orders, and check the scores to 1e-9. A reopened table keeps `fusion = 'rrf'`. Each refusal above.

## 3. Bulk embedding: `inillucent embed`

```sh
inillucent --db app.rdb embed --table chunk --text body --vector v \
  --prefix "search_document: " --device cuda:0
```

What it does:

1. Reads `rowid` and the `--text` column of every row whose `--vector` column `IS NULL`. A rerun
   therefore continues where a stopped run ended. `--all` embeds every row again.
2. Sorts the texts by token count and groups them with `plan_batches`, as `synth-embed` does.
3. Embeds each group with an `OnnxEmbedder` opened on `--device` (`cpu`, `cuda`, `cuda:N`) with
   `--threads` intra op threads. `--sessions N` opens N sessions on the device, default 1. The study's
   documentation measured two sessions on one card as 2.1 times faster than one.
4. Writes the vectors with `UPDATE <table> SET <vector> = ?1 WHERE rowid = ?2`, in transactions of
   `--commit-every` rows, default 1,024. A crash loses at most one transaction.
5. Prints, and in `--output json` returns: rows embedded, rows skipped because the text was `NULL`
   or empty, rows truncated at the model's token limit, seconds, rows a second, device, and the
   `rowid` of up to 20 truncated rows.

Target tables:

- An ordinary table with a `VECTOR(768)` or `BLOB` column: required.
- An `inillucent_search` table, whose hidden `vector` column is the target: check whether `UPDATE`
  of `vector` by `rowid` works on a search table. If it does, support it and test it. If it does not,
  refuse with a message that says to embed an ordinary table and fill the search table with
  `INSERT ... SELECT`, and document that pattern.

The prefix is text the command adds in front of each value, exactly as the SQL
`embed('search_document: ' || body)` does. Default: no prefix, as `embed()`. The documentation shows
`--prefix "search_document: "` for `nomic-embed-text-v1.5`.

Correctness requirement: a vector written by `inillucent embed --device cpu` equals
`embed(prefix || text)` for the same row, byte for byte. With `--device cuda:0` the cosine similarity
to `embed()` is at least 0.99999 for every row. The study measured 0.9999994.

Where the code goes: the command is a row in `registry.rs` and a function in the CLI crate. The CLI
crate already depends on `inillucent-core`, so it opens `OnnxEmbedder` itself. It must respect the
`--root` and `--readonly` rules every other writing command follows: mark it `writes: true`.

When `--device cuda` is asked for and the CUDA runtime is not installed or will not start: fail with
a message naming `inillucent setup-embeddings runtime --gpu`. Never fall back to the CPU quietly.

Measurement to record in `docs/embeddings.md` under "Embedding a whole corpus": rows a second for
`examples/rag-agent`'s 3,696 chunks on the CPU and on `cuda:0`, against `UPDATE ... SET v =
embed(...)` on the CPU. With the commit and date.

## 4. `chunk_text`: a chunking table function

```sql
SELECT d.id, c.seq, c.chunk
FROM doc AS d, chunk_text(d.body, 900, 100, 'Subject: ' || d.subject || char(10) || 'From: ' || d.sender) AS c;
```

| Argument | Meaning | Default |
|---|---|---|
| `text` | the document | required |
| `size` | the largest window, in characters, not counting the heading | 900 |
| `overlap` | characters repeated from the end of one window at the start of the next | 0 |
| `heading` | text written, followed by a blank line, at the start of every chunk. `NULL` writes none | `NULL` |

Output columns: `seq` (0, 1, 2 ...), `chunk` (the heading, a blank line, and the window), `start`
(the character offset of the window in `text`) and `length` (characters in the window).

How a window ends: take `size` characters. If a paragraph break (`\n\n`) falls in the last 30% of
the window, end there. Otherwise a sentence end (`. `, `? `, `! ` or a line break). Otherwise the
last whitespace. Otherwise cut at `size`. Count characters, never bytes, so no window splits a
UTF-8 character. The next window starts `overlap` characters before the end of the previous one,
moved forward to the next whitespace so no window starts in the middle of a word. Every window must
advance by at least one character, so the function always ends.

`NULL` or empty text gives no rows. `size` below 1, `overlap` below 0, or `overlap >= size` is
refused with status `invalid_state` and a message naming the argument.

It is an eponymous virtual table module in `crates/inillucent-ext/src/vtab/chunk.rs`, modeled on
`series.rs`, registered in `Registry::with_builtins`. It needs no model and no feature, so every
build has it. Check that the correlated form above, a table function that reads a column of an
earlier table in the `FROM` list, works in inillucent the way `json_each(d.x)` does. If it does not,
the example must use the form that works, and a separate ticket is filed for the correlated form.

Tests: a text shorter than `size` gives one chunk. Paragraph, sentence and whitespace boundaries
each end a window where described. Overlap repeats the right characters. A text of multibyte
characters (emoji, accented letters) is never split inside a character. The heading is on every
chunk. Every refusal. And the property that joining the windows, minus their overlaps, gives back
the original text.

Add an example to `docs/rag-explained.md` that chunks, embeds with `inillucent embed`, and fills an
`inillucent_search` table, end to end.

## 5. Device and thread settings

Two settings, read in this order: an environment variable, then the value recorded in
`embeddings.json` by `setup-embeddings`, then the default.

| Setting | Environment variable | Recorded by | Default | Applies to |
|---|---|---|---|---|
| thread count | `INILLUCENT_EMBED_THREADS` | `setup-embeddings --threads N` | ONNX Runtime's own choice, as today | `embed()`, `rerank()`, search table reranking |
| device | `INILLUCENT_EMBED_DEVICE` | `setup-embeddings --device cpu\|cuda\|cuda:N` | `cpu`, as today | the same |

The defaults do not change. `docs/embeddings.md` already records that 4 threads was the fastest
single embedding on this machine (21.4 ms against 36.4 ms at the default); say that next to the new
setting as the measured value to try.

`crates/inillucent-search/src/embed.rs` `build()` sets `intra_threads` and `device` on the
`OnnxOptions` from these settings. Follow `residency::configured()` for the reading order. A device
of `cuda` with no CUDA runtime fails the first `embed()` call with the same message as the bulk
command. It never runs on the CPU quietly.

`inillucent setup-embeddings --status` prints both settings and where each came from.

Remove the sentence in `docs/embeddings.md` "The embedder that `embed(TEXT)` uses opens its session
on the processor, and no setting changes that", and describe the device setting instead.

## 6. Truncation reporting

- `embed_tokens(text) -> INTEGER`: the number of tokens `nomic-embed-text-v1.5` sees for `text`,
  before the 1,900 limit is applied. It needs only `tokenizer.json`, so it loads the tokenizer
  without loading the session. Deterministic, `direct_only`, `NULL` for `NULL`, in
  `NEEDS_A_COMPONENT`. A caller finds the rows that will be cut with
  `SELECT id FROM chunk WHERE embed_tokens('search_document: ' || body) > 1900`.
- `inillucent embed` reports the truncated count and rowids, as in section 3.
- `docs/embeddings.md` gains a short section "Text longer than the model reads" that gives the limit,
  the study's 19.5% figure for 6,000 character chunks, and the `embed_tokens` query.

## Documentation to update

Every page that describes behavior this changes, in the same change, per `AGENTS.md` section 2:

| Page | Change |
|---|---|
| `docs/rag-explained.md` | Write the last section, "Doing this in inillucent": install, chunk with `chunk_text`, embed with `inillucent embed`, fill a search table, search with `fusion` and `question`. Every example run against the build, output pasted in |
| `docs/embeddings.md` | reranker install and model, "Reranking" measurements, device and thread settings, bulk command, `embed_tokens`, remove the "no setting changes that" sentence |
| `docs/vector-search.md` | `fusion`, `rerank_depth` and `question` in the option and hidden column tables, the reranked example, the quality numbers from section 1, and the RRF score range |
| `docs/search-explained.md` | one short part on reranking, linking to `rag-explained.md` |
| `docs/sql.md` | `rerank`, `embed_tokens`, `chunk_text` in the function lists |
| `docs/glossary.md` | reranker, cross encoder, RRF, chunk, candidate depth, if missing |
| `docs/README.md` | a link to `rag-explained.md` |
| `docs/roadmap.md` | anything this closes |
| `agent-skills/inillucent-search/SKILL.md` | the recommended RAG recipe; then `node tools/sync-skills.mjs` |
| `examples/rag-agent/rust-example/README.md` | it says inillucent ships no reranker. Correct it |
| `CHANGELOG.md` | the new features, and that an older reader refuses a query naming `question` |
| `drivers/README.md` | only if a binding needs to mention the new functions |
| the inillucent.com documentation chapter | `src/data/documentation.ts` under `sites/inillucent` in the `black-rainbow-labs-sites` repository. Add the same material as `rag-explained.md` in brief. Commit only that file, in that repository |

Then `node tools/doc-style/check.mjs` and `node tools/doc-facts/check.mjs` report no problems, and
`cargo test -p inillucent-compat --test tooling documentation::` passes. Generated files (`docs/pragmas.md`,
counts of commands and functions) are regenerated with the tools that own them, never edited by hand.

## Tests and where they go

Follow `tests/inillucent-testing-tdd.md`. Every new test file gets a row in `tests/selection.toml`.

| Test | Tier | Needs a model |
|---|---|---|
| `fusion` option: parse, refusals, persistence across reopen, the RRF worked example | engine | no |
| `chunk_text`: every case in section 4 | engine | no |
| `rerank_depth` parse and refusals; `question` refused without MATCH or vector | engine | no |
| `rerank`, `embed_tokens` and `question` answer `unsupported` in a build without `embed` | engine | no |
| `rerank()` scores a relevant passage above an unrelated one, returns values in 0 to 1, `NULL` in, `NULL` out | retrieval | reranker |
| `OnnxReranker` agrees with the study's Python scores to 1e-4 (store the 20 expected scores in the test) | retrieval | reranker |
| a search with `question` reorders rows by reranker score, returns `k` rows, `score` in 0 to 1 | retrieval | reranker |
| `inillucent embed --device cpu` writes vectors equal to `embed()` byte for byte, resumes after a stop, reports truncation | retrieval | embedder |
| `inillucent embed --device cuda:0` agrees with `embed()` to cosine 0.99999 | retrieval | embedder and CUDA; declared absent where there is no card |
| `embed_tokens` counts match the tokenizer, and flags a text over 1,900 tokens | retrieval | tokenizer |
| `command_parity` passes with the new command and parameters | tooling | no |
| the capability table matches the engine | driver capability test | no |

A test that needs the reranker skips when it is absent. On this machine install it
(`inillucent setup-embeddings reranker`) so the tests run. Run the final check with `--strict`, and
declare a name absent only when this machine cannot have it.

## Order of work

1. `fusion` option (small, no model). Tests. Commit.
2. `chunk_text` (no model). Tests. Commit.
3. Device and thread settings in `embed.rs` and `setup-embeddings`. Commit.
4. `embed_tokens` and the truncation counts. Commit.
5. `inillucent embed`. Tests on CPU and GPU. Measurement. Commit.
6. The reranker: manifest, install, `OnnxReranker`, `rerank()`, search table reranking, command line
   flag, capability row. Tests. The three measurements. Commit.
7. Documentation, skills, site chapter, `CHANGELOG.md`. Commit.
8. `target/debug/inillucent-testrun --changed origin/main --strict` exits 0. `cargo fmt`. The
   documentation checks pass.

Commit after each step on branch `task-2158`, with messages that start `task-2158:`.

## Risks

| Risk | What to do |
|---|---|
| ONNX Runtime 1.22.0 cannot run the ModernBERT graph | The study ran the same file through Python ONNX Runtime. Check the version it used (`.venv` in `tools/rag-lab`). If 1.22.0 refuses an operator, report the operator and the version in a ticket comment, and ship the reranker with the smallest runtime change that works, updating the pinned runtime digests |
| The manifest digest changes for existing caches | The test in section 1 that the digest of `nomic_v1_5()` is unchanged |
| A reranked search on the CPU takes 10 seconds and a user thinks it hung | Document the CPU cost next to every example. `rerank_depth` lets a user trade depth for time |
| A GPU request falls back to the CPU and runs 30 times slower with no message | `error_on_failure()` is already set. The tests check that a failed CUDA start is an error |
| An older reader opens a table with `rerank_depth` or `fusion` | It ignores unknown `%_config` keys. `fusion = 'rrf'` would then search with the adaptive weight on the older reader. Note it in `CHANGELOG.md` |

## Done means

- All six features work, with their tests, on this machine, with the reranker and the CUDA runtime
  installed.
- The measurements in sections 1 and 3 are recorded with their commit and date.
- `docs/rag-explained.md` ends with a working recipe whose output was pasted from a real run.
- `inillucent-testrun --changed origin/main --strict` exits 0, and the documentation checks pass.
