# inillucent documentation

This folder holds the reference pages for inillucent. Each page can be read on its own.

To learn inillucent step by step, read the documentation book at
[inillucent.com/docs](https://inillucent.com/docs). The book is a tutorial in 26 chapters. The pages
here are for looking things up.

The tables below list the pages in reading order for a programmer who has used a database but has
never written one. Start with what inillucent is, then the terms, then how to run it, then how the
two engines work.

`cargo test -p inillucent-compat --test tooling documentation::` fails when a page in `docs/` is missing from
this index.

## Start here

| | Page | What it covers |
|---|---|---|
| 1 | [Product overview](product-overview.md) | What inillucent is, who it is for, and how it compares with PostgreSQL and pgvector |
| 2 | [Glossary](glossary.md) | Each database and search term the pages use, explained in one sentence |
| 3 | [Getting started](getting-started.md) | Installing, the four programs, a first database, and the exit codes |
| 4 | [Architecture overview](architecture-overview.md) | The two engines in one file, and one query that uses both |

## The two engines

| | Page | What it covers |
|---|---|---|
| 5 | [The retrieval engine](architecture.md) | How vector search and keyword search work, with no Rust code |
| 6 | [The relational engine](relational-architecture.md) | How the SQL engine stores data, runs transactions, writes its log, recovers after a crash and makes backups |
| 7 | [Where the vectors live](vector-residency.md) | Holding vectors in memory or reading them from the file, and what each choice costs |

## Using it

| | Page | What it covers |
|---|---|---|
| 8 | [SQL support](sql.md) | The SQL that runs, the cases that differ from SQLite, and the statements the engine refuses |
| 9 | [Pragmas](pragmas.md) | Every pragma the engine recognises. A program writes this page from the engine's own list |
| 10 | [Search explained from the start](search-explained.md) | Keyword search, search by meaning and hybrid search for a reader who has never built a search, with every term defined and examples to run |
| 11 | [Retrieval for RAG, explained from the start](rag-explained.md) | What a RAG system is, each step and the choices at each step, and how to set it up in inillucent: chunking, a reranker, reciprocal rank fusion and bulk embedding |
| 12 | [Vector search](vector-search.md) | `VECTOR(N)` columns, HNSW indexes, the `inillucent_search` table, and how hybrid search ranks results |
| 13 | [Embeddings](embeddings.md) | Running the embedding model inside your process, installing it, and choosing a model |
| 14 | [Migrating](migrating.md) | Copying a SQLite file, a PostgreSQL database or a MySQL database into inillucent |
| 15 | [Encryption at rest](encryption.md) | Keys, what is encrypted, how to encrypt a database you already have, and what it costs |

## Measurements

| | Page | What it covers |
|---|---|---|
| 16 | [Performance](performance.md) | Current speed, processor time, memory and file size against SQLite 3.53.4, overall and by category, and the workloads that are slower. Every earlier run and each hill climb is in [Performance history](performance-history.md) |
| 17 | [Feature comparison](feature-comparison.md) | The full 416 case differential probe against SQLite, feature by feature, and the retrieval engine against pgvector |
| 18 | [Retrieval quality](retrieval-quality.md) | The 17 graded comparisons with PostgreSQL and pgvector, and how a measurement becomes a verdict |
| 19 | [Synthetic corpus](../tests/synthetic-corpus.md) | How to build the public corpus that every retrieval number is measured on |

`inillucent-scorecard.md` in the repository root is the full score card from the grading run. It has
every interval, every p-value and every diagnostic. `inillucent-bench grade` writes
`inillucent-scorecard.md` there, and writes the same measurements to `inillucent-scorecard.json`.
[Retrieval quality](retrieval-quality.md) and `inillucent-scorecard.md` describe the same run, taken
on 2026-09-20.

## Research

Each study measures one question about how inillucent should work, and says what was built because
of each result. The same studies are at [inillucent.com/research](https://inillucent.com/research).

| | Page | What it covers |
|---|---|---|
| 20 | [Research](research/README.md) | The list of studies, newest first |
| 21 | [Which retrieval techniques find the right text for RAG](research/rag-retrieval-study.md) | Chunking, headings, embedding models, fusion, rerankers and language model helpers, measured on 67,369 emails, and the features each result led to |

## Working on it

| | Page | What it covers |
|---|---|---|
| 22 | [Repository and building](repository.md) | The crates and what each one does, building, running the tests, and the test coverage table |
| 23 | [Dependency policy](dependency-policy.md) | Which crates a production crate may use, and why the allowed list is short |
| 24 | [Writing style](writing-style.md) | How to write and check a page in this folder or a chapter on inillucent.com |
| 25 | [Roadmap](roadmap.md) | What is not built yet, in the order it is being worked on |
| 26 | [Closed items](closed-items.md) | What came off the roadmap, and the measurement that closed each item |
| | [`AGENTS.md`](../AGENTS.md) | The starting page for an AI agent that uses or changes this repository |
| | [`agent-skills/`](../agent-skills/README.md) | One page for each common job, readable by any AI agent |

## Data files

Two files in this folder are data read by programs and tests.

| File | What it holds |
|---|---|
| [`invariants/layering.toml`](invariants/layering.toml) | Which crate may depend on which, and which outside crates each one may use. A test enforces it. [Dependency policy](dependency-policy.md) explains it |
| [`reference-register.toml`](reference-register.toml) | Every outside project consulted while building inillucent, such as the SQLite file format, and how each one was used |
