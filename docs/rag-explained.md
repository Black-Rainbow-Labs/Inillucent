# Retrieval for RAG, explained from the start

This page explains how an application finds the right text to give a language model, which is the
first half of what is called RAG. It defines every term, walks through each step and the choices at
each step, and says what each choice measured on a real collection of 67,369 emails. The last
section shows how to set it up in inillucent, with every command and its output.

The study behind the numbers has its own page,
[Which retrieval techniques find the right text for RAG](research/rag-retrieval-study.md). It covers
how the questions were written and graded, every comparison with its interval, the round by round
tuning, and why each feature was built the way it was. The same material is chapter 18 of the book
at [inillucent.com/docs](https://inillucent.com/docs#rag) and the study at
[inillucent.com/research](https://inillucent.com/research#rag-retrieval).

You need to know what a table and a `SELECT` are. You do not need any mathematics beyond adding and
dividing. [Search explained from the start](search-explained.md) covers keyword search and search by
meaning in more detail, and is a good page to read first.

## Terms used on this page

Read this table once now and come back to it when a word is unfamiliar. Each term is explained again
where the page first uses it.

| Term | Meaning |
|---|---|
| **RAG** | retrieval augmented generation. A program searches its own text for passages that answer a question, then gives those passages to a language model so the model can answer from them |
| **retrieval** | the search step of RAG. This page is about retrieval |
| **language model** | a program that writes text, such as a chat assistant. Also called an LLM |
| **agent** | a language model that can call tools, such as a search tool, and decide what to do next from the results |
| **document** | one item in the collection. In the study on this page a document is one email |
| **chunk** | a piece of a document, cut to a fixed size so that one search result is short enough to read |
| **heading** | a short line at the start of every chunk that says which document the chunk came from. In the study it was the email's subject, sender and date |
| **keyword search** | finding chunks that contain the words of the question. Scored with BM25 |
| **BM25** | the usual formula for scoring a keyword match. Rare words count for more than common ones |
| **embedding** | a list of numbers that a model makes from a piece of text. Texts with similar meaning get similar lists |
| **embedding model** | the model that makes embeddings. inillucent runs `nomic-embed-text-v1.5` |
| **vector search** | finding chunks whose embeddings are closest to the question's embedding. Also called search by meaning |
| **hybrid search** | running keyword search and vector search for the same question and combining the two lists into one |
| **fusion** | the rule that combines the two lists of a hybrid search into one list |
| **RRF** | reciprocal rank fusion. A fusion rule that uses only each chunk's position in each list |
| **reranker** | a second model that reads the question and one chunk together and gives the pair a score. It is slower than an embedding model and more accurate |
| **candidates** | the chunks a first search returns for the reranker to score. The study used 60 |
| **query rewriting** | asking a language model to write other versions of the question and searching for each |
| **HyDE** | hypothetical document embeddings. A language model writes an imagined answer, and the search looks for chunks close to that imagined answer |
| **GPU** | a graphics card. Models run on it many times faster than on the main processor, the CPU |

The measurement terms are defined in [How the study measured this](#how-the-study-measured-this).

## What retrieval does

```mermaid
flowchart LR
    Q["A question"] --> S["Search the chunks"]
    S --> R["The best 5 or 10 chunks"]
    R --> M["Language model"]
    Q --> M
    M --> A["An answer that cites the chunks"]
```

A language model knows only what it was trained on. It has never seen your email, your wiki or your
support tickets. RAG solves this by searching your text first and pasting the best passages into the
model's input, next to the question. The model then answers from those passages.

The answer can only be as good as the passages. If the search misses the email that holds the
answer, the model either says it does not know or, worse, makes something up. So the quality of the
search sets a ceiling on the quality of the answer. The study on this page measured that search.

## The steps of a retrieval system

A retrieval system has two halves. The first half runs once, when text is added. The second half
runs for every question.

**When text is added:**

1. **Chunking.** Each document is cut into chunks. The study cut emails into chunks of about 900
   characters, which is a few paragraphs.
2. **Heading.** A short heading is written at the start of each chunk, such as
   `Subject: Q3 budget review / From: Dana / Date: 2024-03-02`. Without it, a chunk from the middle of
   a long email has no idea what it belongs to.
3. **Embedding.** The embedding model turns each chunk into a list of 768 numbers.
4. **Indexing.** The chunk text goes into a keyword index and the numbers go into a vector index. An
   `inillucent_search` table holds both.

**When a question arrives:**

1. **First search.** Keyword search and vector search each return a list of the best chunks.
2. **Fusion.** The two lists are combined into one.
3. **Reranking**, if the system has a reranker. The top chunks of the combined list are scored again
   by the reranker, and reordered by that score.
4. **Return** the best 5 or 10 chunks to the language model.

Every one of those steps has choices. The study tried the common ones. This table is the short
answer, and the sections after it explain each choice.

| Step | The choices | What the study found | Start with |
|---|---|---|---|
| Chunking | 450, 900, 2,000 or 6,000 characters | No measurable difference | 900 characters with an overlap of 100 |
| Heading | a heading on each chunk, or none | Removing it cost 0.043 to 0.111 nDCG@10 | Always a heading |
| Embedding | the model inside inillucent, or a newer one in a separate program | No measurable difference | `nomic-embed-text-v1.5` through `inillucent embed` |
| First search | keyword, vector or hybrid | Hybrid beat keyword alone by 0.038 | Hybrid |
| Fusion | adaptive weight, fixed weight or RRF | RRF won on the emails, the adaptive weight on another collection | Measure both |
| Reranking | none, or a reranker over the top 30, 60 or 100 | +0.10 to +0.13, the largest gain | A reranker over the top 60 on a graphics card |
| Help from a language model | query rewriting, HyDE, document summaries | No gain once a reranker ran | None |

## The techniques that were tested

### Chunk size

A small chunk is precise: when it matches, the match is about that exact passage. A large chunk
keeps more context together, so an answer that spans three paragraphs is in one place. An embedding
model also has a limit on how much text it reads. `nomic-embed-text-v1.5` reads about 1,900 tokens
(roughly 7,000 characters of English) and ignores the rest.

The study compared chunks of 450, 900, 2,000 and 6,000 characters.

### The chunk heading

The study compared 900 character chunks with the heading and the same chunks without it.

### The embedding model

The collection was first indexed with Nomic v2, a newer embedding model served by a separate
program. The study compared it with `nomic-embed-text-v1.5`, which inillucent runs inside the
database with the SQL function `embed()`. Using the model inside the database means one less program
to install and run.

### Keyword, vector and hybrid search

Keyword search is good at exact names, numbers and rare words: `invoice 4471`, `parse_headers`, a
person's surname. It fails when the question uses different words from the text: a question about
"a job offer" will not find an email that says "we'd like to extend a position".

Vector search is good at meaning and fails at exact tokens. It finds the "extend a position" email,
and it may rank an email about invoice 4470 above the one about invoice 4471.

Hybrid search runs both, so each covers the other's failures.

### Fusion: how the two lists are combined

The two lists have scores on different scales. A BM25 score can be 14.2 and a vector similarity 0.83,
so they cannot simply be added. The study compared three fusion rules:

- **Adaptive weight.** This is inillucent's default. Each list's scores are scaled to run from 0 to
  1, then added with a weight. The weight starts at 0.35 for the vector list and moves between 0.05
  and 0.95 depending on the words in the question. A question full of identifiers moves it toward
  keywords.
- **Fixed weight 0.5.** The same scaling, with the two lists always weighted equally.
- **RRF.** Scores are ignored. A chunk gets `1 / (60 + position)` from each list it appears in, and
  the two numbers are added. A chunk at position 1 in both lists gets `1/61 + 1/61`. A chunk at
  position 1 in one list only gets `1/61`. The 60 is a constant from the paper that introduced RRF.
  It keeps the first few positions from dominating.

Here is RRF on a small example. Keyword search returned chunks A, B, C in that order. Vector search
returned C, D, A.

| Chunk | Keyword position | Vector position | RRF score |
|---|---|---|---|
| A | 1 | 3 | 1/61 + 1/63 = 0.0323 |
| C | 3 | 1 | 1/63 + 1/61 = 0.0323 |
| B | 2 | none | 1/62 = 0.0161 |
| D | none | 2 | 1/62 = 0.0161 |

A and C appeared in both lists, so they come first.

### Reranking

An embedding model reads the question and each chunk separately. It turns each into numbers, and
the search compares the numbers. It never sees the question and the chunk side by side. That is what
makes it fast: every chunk is embedded once, ahead of time.

A reranker reads the question and one chunk together, as one input, and answers with one score for
how well the chunk answers the question. Because it sees both at once, it can notice that the
question asks about a date in 2021 and the chunk is about 2019. It is much slower, because it has to
run once for every pair, at search time. So a system runs it only on the top candidates of a fast
first search.

The study tested two rerankers: `bge-reranker-v2-m3` (568 million parameters) and
`gte-reranker-modernbert-base` (149 million parameters). It reranked the top 30, 60 and 100
candidates.

### Asking a language model to help the search

Three techniques use a language model before or during indexing:

- **Query rewriting.** The model writes three other versions of the question, the system searches
  for each, and the lists are fused.
- **HyDE.** The model writes an imagined answer to the question. The system searches for chunks
  close to the imagined answer, on the theory that an answer looks more like another answer than a
  question does.
- **Document summaries.** When the text is indexed, the model writes a one sentence summary of each
  whole email, and the summary is added to every chunk of that email. This gives each chunk context
  from the rest of its document.

### The agent

Finally, the study gave a local language model two tools, `search_email` and `read_document`, and
asked it every question. The model decided what to search for, how many times, and which documents
to read. This measured whether better search produces better answers, which is what a user sees.

## How the study measured this

119 questions were written from real emails and split into a **dev** half, used for every decision,
and a **test** half, only reported. A language model graded 11,138 question and document pairs from
0 to 2, and was checked against known answers. Each search was scored with **nDCG@10**, which is 1
when the best documents come first and 0 when nothing useful is found. Every difference has a 95%
interval, and a difference whose interval crosses zero is **inside the noise**: the study could not
tell the two techniques apart.

The main results, as changes in nDCG@10:

| Technique | Change | 95% interval |
|---|---|---|
| A reranker over the top 60, against hybrid search | **+0.106** | +0.070 to +0.147 |
| Heading removed, with a reranker | **-0.111** | -0.148 to -0.073 |
| RRF, against the adaptive weight | **+0.035** | +0.006 to +0.066 |
| Hybrid search, against keyword search alone | **+0.038** | +0.021 to +0.058 |
| `nomic-embed-text-v1.5` inside inillucent, against Nomic v2 | -0.002 | -0.017 to +0.014 |
| 6,000 character chunks, against 900 | -0.003 | -0.031 to +0.026 |
| HyDE, against hybrid search | +0.005 | -0.034 to +0.046 |

On the whole mailbox, hybrid search scored 0.635 and reranking over RRF scored 0.758. A reranker
over 60 candidates took 0.14 to 0.23 seconds on a graphics card and 10.6 to 22.8 seconds on 8
processor threads. Embedding all 558,429 chunks took 22 minutes on a graphics card and would take
about 12 hours through `embed()` on the processor. The agent that answered from the search gave
0.780 correct answers on the test half against 0.771 before, which is inside the noise, and made
0.49 fewer tool calls per answer.

[Which retrieval techniques find the right text for RAG](research/rag-retrieval-study.md) has every
comparison, the method, the grading checks and the limits of the study.

## What the results mean

1. **Use a reranker.** It was the largest gain by far, about +0.10 to +0.13 nDCG@10 on every
   collection and every embedding model. It needs a graphics card to be fast enough for a person
   who is waiting. The smaller `gte-reranker-modernbert-base` scored the same as
   `bge-reranker-v2-m3` and is faster.
2. **Measure the fusion rule on your own questions.** On the mailbox, RRF or a fixed weight of 0.5
   beat the adaptive weight by 0.035 to 0.041. On the corpus inillucent's own score card uses, the
   adaptive weight won: nDCG@10 0.982 against 0.917 for RRF and 0.955 for a fixed 0.5, on document
   title questions (see `inillucent-scorecard.md` in the repository). So the best rule depends on the
   collection, and inillucent keeps the adaptive weight as its default. With a reranker the fusion
   rule matters much less: on the whole mailbox, reranking over RRF scored 0.758 and reranking over
   the adaptive weight 0.749, inside the noise.
3. **Put a heading on every chunk.** Removing it cost 0.043 without a reranker and 0.111 with one.
   The reranker reads the chunk as the only evidence of what it is about, so a chunk that does not
   say which email it came from loses the most.
4. **Chunk size does not matter much.** From 450 to 6,000 characters nothing changed by more than
   the noise. 900 characters is a reasonable default. Chunks longer than about 7,000 characters are
   cut off by `nomic-embed-text-v1.5`: 19.5% of the 6,000 character chunks were cut.
5. **The embedding model inside inillucent is good enough.** `nomic-embed-text-v1.5` through
   `embed()` matched a newer model served by a separate program, and searches were faster because
   there is no network call.
6. **Language model tricks at search time did not help.** Query rewriting and HyDE changed nothing
   measurable, and they add a language model call to every search.
7. **Document summaries help only without a reranker.** They added +0.063 to hybrid search and
   +0.023 (inside the noise) once a reranker ran. Writing a summary for every document in a large
   collection costs hours of language model time.
8. **Embed a large collection on a graphics card.** 22 minutes against 12 hours.

## Doing this in inillucent

Every command and every result below was run on 29 September 2026 at commit `ed70d251`, on the 80
Wikipedia articles of `examples/rag-agent/corpus`. The chunks were embedded on a graphics card, card
0. On a machine with no card, leave out `--device cuda:0` and expect the embedding step to take
minutes instead of seconds.

The recipe follows the order of the study: chunk with a heading, embed in bulk, search with both
lists, and rerank. The last part shows how a program keeps the search table current as documents are
added, changed and deleted, using a mailbox.

```mermaid
flowchart LR
    A["Documents"] --> B["chunk_text: chunks with a heading"]
    B --> C["inillucent embed: one vector per chunk"]
    C --> D["inillucent_search table: keywords and vectors"]
    D --> E["question = ...: the reranker reorders the top 60"]
```

### Install what it needs

```sh
inillucent setup-embeddings all             # the runtime and the embedding model, about 620 MB
inillucent setup-embeddings reranker        # the reranker, about 600 MB
inillucent setup-embeddings runtime --gpu   # only for a graphics card. It needs CUDA installed
```

`inillucent setup-embeddings --status` says what is installed. The reranker is a separate download
because a program that never reranks should not carry 600 MB. [Embeddings](embeddings.md) explains
each command.

### Chunk the documents with a heading on every chunk

The study lost 0.043 to 0.111 nDCG@10 when chunks had no heading. `chunk_text(text, size, overlap,
heading)` cuts a document into windows and writes the heading at the top of each one. Load the
documents into a table, then chunk them with a join:

```sh
inillucent create app.rdb
inillucent --db app.rdb import articles.csv --table article
```

```
imported 80 rows into article
```

```sql
CREATE TABLE chunk (id INTEGER PRIMARY KEY, article_id INTEGER, seq INTEGER, body TEXT, v VECTOR(768));

INSERT INTO chunk (article_id, seq, body)
SELECT a.rowid, c.seq, c.chunk
FROM article AS a, chunk_text(a.body, 900, 100, 'Title: ' || a.title) AS c;
```

```
ok. 3233 rows changed.
```

The four arguments are the text, the largest window in characters, how many characters each window
repeats from the end of the one before, and the heading. A window ends at a paragraph break, a
sentence end or a space, in that order of preference, when one falls in its last 30%. The other
columns are `start`, the offset of the window in the text, and `length`, the number of characters in
it. A `NULL` or empty text gives no rows. Write a call with the arguments you mean: a literal
`NULL` in an argument position makes the query return no rows, so leave the heading out when a
document has none.

### Find the chunks the embedding model would cut

```sql
SELECT count(*) AS chunks,
       max(embed_tokens('search_document: ' || body)) AS most_tokens,
       sum(embed_tokens('search_document: ' || body) > 1900) AS over_the_limit
FROM chunk;
```

```
chunks  most_tokens  over_the_limit
------  -----------  --------------
3233    488          0
```

The embedding model reads at most 1,900 tokens and ignores the rest without a message. In the study,
19.5% of 6,000 character chunks were over the limit. Chunks of 900 characters are far under it.

### Embed the whole table

```sh
inillucent --db app.rdb embed --table chunk --text body --vector v --prefix "search_document: " --device cuda:0
```

```
embedded 3233 rows in 5.1 s, 640 rows a second, on cuda:0 with 1 session. 0 skipped because the text was NULL or empty. 0 cut at the model's token limit
```

This is the first import of a whole collection. Documents that arrive later are embedded one at a time
in the statement that stores them, as [Keep the search table current](#keep-the-search-table-current)
shows. `inillucent embed` reads the rows whose `v` is `NULL`, so a run that stops continues where it ended.
The prefix `search_document: ` belongs to `nomic-embed-text-v1.5`. `--sessions 2` runs two sessions on the
card. [Embeddings](embeddings.md#embedding-a-whole-table) has the measured speeds, including 11
rows a second for `embed()` in SQL and 813 for two sessions on a card.

### Search with both lists

```sql
CREATE VIRTUAL TABLE chunk_search USING inillucent_search(body, dims = 768, fusion = 'rrf', rerank_depth = 60);

INSERT INTO chunk_search (rowid, body, vector) SELECT id, body, v FROM chunk;
```

The heading is part of `body`, so the keyword list and the reranker both see it. `fusion = 'rrf'`
combines the two lists with reciprocal rank fusion, and `rerank_depth = 60` is how many rows the
reranker will score. Neither is the default. The default fusion is the adaptive weight, and a table
that names no `rerank_depth` uses 60. Two collections disagreed about which fusion is better, so measure
both on your own questions.

```sql
SELECT s.rowid, a.title, round(score(chunk_search), 4) AS score
FROM chunk_search AS s
JOIN chunk AS c ON c.id = s.rowid
JOIN article AS a ON a.rowid = c.article_id
WHERE chunk_search MATCH 'taught OR world OR made OR water'
  AND vector = embed('search_query: who taught that the world is made of water')
  AND k = 5
ORDER BY rank;
```

```
rowid  title              score
-----  -----------------  ------
2974   Thales of Miletus  0.0328
611    Classical element  0.0265
584    Classical element  0.0263
154    Anaximander        0.0259
3000   Thales of Miletus  0.0259
```

The score of a reciprocal rank fusion search is at most 2/61, about 0.033. The best row here is first
in both lists and scores `1/61 + 1/61`. Do not set a threshold on it expecting 0 to 1.

### Rerank

Add `question`, the question in plain words:

```sql
SELECT s.rowid, a.title, round(score(chunk_search), 4) AS score
FROM chunk_search AS s
JOIN chunk AS c ON c.id = s.rowid
JOIN article AS a ON a.rowid = c.article_id
WHERE chunk_search MATCH 'taught OR world OR made OR water'
  AND vector = embed('search_query: who taught that the world is made of water')
  AND question = 'who taught that the world is made of water'
  AND k = 5
ORDER BY rank;
```

```
rowid  title              score
-----  -----------------  ------
2960   Thales of Miletus  0.8667
2974   Thales of Miletus  0.8397
2973   Thales of Miletus  0.8228
2976   Thales of Miletus  0.8228
127    Anaximander        0.8208
```

The search collected the top 60 rows of the fused list, the reranker scored them in one call, and the
five highest came back. `score` is now the reranker's score from 0 to 1. The question has no
`search_query: ` label, because that label is for the embedding model and a cross encoder was not
trained with it.

Reranking costs time. On the processor, 60 candidates took 5 to 8 seconds on this machine, and on the
card about 0.07 seconds. If a machine has no card, lower `rerank_depth`: 20 candidates take about a
third as long. Every example here ran on the card. [Embeddings](embeddings.md#reranking) has the
measurements.

The study found the reranker raised nDCG@10 by 0.10 to 0.13 on every collection. On the 80 articles
here it made a smaller difference. [Vector search](vector-search.md#reranking-a-search) prints the
comparison. A reranker helps most where many chunks look like good answers and only a few are, which is
what a mailbox of similar emails is. Check it on your own questions.

### Keep the search table current

The steps above build a table from a collection that already exists. After that, documents arrive a
few at a time. Each one is embedded once, in the statement that stores it, and a search embeds only
the question. Nothing is embedded again unless its text changes.

This example is a mailbox. `email` holds each message. `email_search` holds the chunks, and each chunk
carries its email's id and sender as `FACET` columns, so a search can filter on them and a program can
delete or join by them. The `inillucent_search` table stores its own copy of each chunk's text and
vector in the database file, so it is built once and survives a restart.

```sql
CREATE TABLE email (id INTEGER PRIMARY KEY, sender TEXT, subject TEXT, sent_at TEXT, body TEXT);

CREATE VIRTUAL TABLE email_search USING inillucent_search(text, email_id FACET, sender FACET, dims = 768);
```

**A new email arrives.** Store it, then cut it into chunks and embed each chunk in one statement.
Run both in one transaction, so a failure leaves neither:

```sql
BEGIN;
INSERT INTO email (id, sender, subject, sent_at, body) VALUES (?1, ?2, ?3, ?4, ?5);

INSERT INTO email_search (text, email_id, sender, vector)
SELECT c.chunk, e.id, e.sender, embed('search_document: ' || c.chunk)
FROM email AS e,
     chunk_text(e.body, 900, 100,
       'Subject: ' || e.subject || char(10) || 'From: ' || e.sender || char(10) || 'Date: ' || e.sent_at) AS c
WHERE e.id = ?1;
COMMIT;
```

The heading gives every chunk its email's subject, sender and date. Each chunk costs one call to the
model, 12 to 36 ms on the processor, and the first call in a process loads the model. Three short
emails, each one chunk, took 2.1 seconds on the processor including that load.

**Search.** Join the hits back to `email` by the facet:

```sql
SELECT e.id, e.subject,
       round(confidence(email_search), 3) AS confidence,
       origin(email_search) AS origin
FROM email_search
JOIN email AS e ON e.id = email_search.email_id
WHERE email_search MATCH 'job OR offer'
  AND vector = embed('search_query: did anyone offer me a job')
  AND k = 3
ORDER BY rank;
```

```
id  subject                 confidence  origin
--  ----------------------  ----------  ------
2   Offer letter            0.497       both
1   Invoice 4471 for March  0.171       vector
3   Dinner Sunday           0.148       vector
```

The offer letter says "we would like to extend you a position", and the search by meaning found it
beside the keyword `offer`. Add `AND sender = 'hr@globex.com'` to search one sender's mail.

**An email changes.** The number of chunks can change with the text, so delete the email's chunks and
insert them again. Only that email is embedded again:

```sql
BEGIN;
UPDATE email SET body = ?2 WHERE id = ?1;
DELETE FROM email_search WHERE email_id = ?1;
-- then the INSERT ... SELECT ... chunk_text ... embed from above, for WHERE e.id = ?1
COMMIT;
```

**An email is deleted.**

```sql
BEGIN;
DELETE FROM email_search WHERE email_id = ?1;
DELETE FROM email WHERE id = ?1;
COMMIT;
```

A facet is stored as text and has text affinity, so `email_id = 1` and `email_id = '1'` select the
same chunks. Release 2.0.1 and earlier compared a facet with an integer under no affinity outside a
search, so `DELETE FROM email_search WHERE email_id = 1` deleted nothing and gave no error. On those
releases write `email_id = CAST(?1 AS TEXT)`.

**A whole mailbox the first time.** Insert the chunks without a vector, then fill every vector in one
run, on a graphics card if there is one:

```sql
INSERT INTO email_search (text, email_id, sender)
SELECT c.chunk, e.id, e.sender
FROM email AS e,
     chunk_text(e.body, 900, 100,
       'Subject: ' || e.subject || char(10) || 'From: ' || e.sender || char(10) || 'Date: ' || e.sent_at) AS c;
```

```sh
inillucent --db mail.rdb embed --table email_search --text text --vector vector --prefix "search_document: " --device cuda:0
```

That is the same command as [Embed the whole table](#embed-the-whole-table), pointed at the search
table's own `vector` column. After the first import, go back to embedding each new email as it
arrives.

**Why a trigger cannot do the embedding.** `embed(TEXT)` is refused inside a trigger, a view, a
generated column, a `DEFAULT` and a `CHECK`, with `embed may only be used from top-level SQL`. A
database file must not be able to make the program that opens it load a model on every insert. A
trigger can copy a vector your statement computed, as
[Vector search](vector-search.md#keeping-an-fts5-table-in-step-with-a-table) shows, at the cost of
storing each vector twice.

### From the command line, and what to measure next

`inillucent search "taught OR world OR made OR water" --table chunk_search --rerank` runs the same
reranked search from the command line. The MCP tool `inillucent_search` takes the same `rerank`
parameter.

Three things to measure on your own collection, in the order the study ranked them:

1. **Rerank.** Compare a search with `question` and without it, on questions you have graded.
2. **The heading.** Chunk with and without a heading and compare.
3. **The fusion.** Compare `fusion = 'rrf'`, `'adaptive'` and `'weighted'` with a `vector_weight` of 0.5.
