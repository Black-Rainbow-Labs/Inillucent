# Which retrieval techniques find the right text for RAG

Study run on 29 September 2026. Published at
[inillucent.com/research](https://inillucent.com/research#rag-retrieval).

We measured which search techniques put the right passages in front of a language model, on a
mailbox of 67,369 emails with 119 graded questions. A reranker was the largest gain by far. A
heading on every chunk mattered. Chunk size, rewriting the question and HyDE did not. inillucent now
has a feature for each technique that won.

| Documents | Chunks | Graded questions | Graded pairs |
|---|---|---|---|
| 67,369 | 558,429 | 119 | 11,138 |

Every term on this page is defined in [Retrieval for RAG, explained from the start](../rag-explained.md#terms-used-on-this-page).

## The short version

RAG answers a question in two steps: search your own text for passages, then give them to a
language model. The answer can only be as good as the passages, so this study measured the search.

We tested chunk sizes, a heading on each chunk, two embedding models, three ways to combine keyword
and vector search, two rerankers, and three ways of asking a language model to help the search. Then
we gave a local language model a search tool and graded its answers.

| Finding | Measured | What we did |
|---|---|---|
| [A reranker is the largest gain](#result-rerank-the-top-candidates) | +0.10 to +0.13 nDCG@10 on every collection | Built `rerank()` and the `question` column of `inillucent_search` |
| [A heading on every chunk matters](#result-keep-the-heading-on-every-chunk) | Removing it cost 0.043, or 0.111 with a reranker | Built `chunk_text` with a heading argument |
| [RRF beat the default fusion on the emails](#result-how-to-combine-keyword-and-vector-search) | +0.035 nDCG@10 | Added `fusion = 'rrf'`. Kept the default, because another collection disagreed |
| [The built in embedding model is as good as a newer one](#result-the-built-in-embedding-model-is-good-enough) | +0.004 on the whole mailbox, inside the noise | Kept `nomic-embed-text-v1.5` |
| [Embedding a large collection needs a graphics card](#result-the-built-in-embedding-model-is-good-enough) | 22 minutes against about 12 hours | Built `inillucent embed` with `--device cuda:0` |
| [Chunk size made no difference](#result-chunk-size-did-not-matter) | Every interval crosses zero | Built `embed_tokens` so a chunk cut at the token limit is never silent |
| [Asking a language model to help the search did not pay](#result-asking-a-language-model-to-help-the-search) | Query rewriting +0.002, HyDE +0.005, both inside the noise | Built nothing for them |
| [Better search did not give more correct agent answers](#result-what-the-agents-answers-showed) | +0.008 pass rate, inside the noise | Reported it. The agent needed fewer tool calls |

## Why we ran it

An email search application built on inillucent was moving from an embedding model served by a
separate program to the model inillucent runs inside the database. Before rebuilding its index, we
wanted to answer two questions. Would the switch lose quality? And which RAG techniques are worth
building into inillucent, so that every user gets them without writing them?

Most published advice on RAG comes from public collections of articles with questions written for
them. A mailbox is harder. Many emails look alike, exact names and numbers matter, and people
remember them only vaguely. We measured on a mailbox because that is the kind of text people most
want to search.

## The collection and the questions

The collection was one person's mailbox: 67,369 emails and attachments, cut into 558,429 chunks of
about 900 characters. Building an index takes time, so most comparisons ran on a fixed sample of
15,000 documents, and the important ones were checked again on the whole mailbox. The sample kept
every email a question was written from, the rest of its thread, and random documents. That way the
right answer always had many similar emails around it, as it does in a real mailbox.

Claude Opus 5.5, a language model, wrote 119 questions from real emails, the way a person would type
them months later from memory, without copying the email's own phrases. They came in three styles: a
few keywords, a full question, and a vague recollection such as "that thing someone sent about the
move". They came in three kinds: a fact in an email, a fact in an attachment, and a topic spread
across several emails.

The questions were split at random into a **dev** half of 60 and a **test** half of 59. Every
decision was made by looking at the dev half only. The test half was scored and reported, and never
used to choose. A technique that helps on the dev half and not on the test half was tuned to those
particular questions and would not help a real user. [The agent result](#result-what-the-agents-answers-showed)
shows why this matters.

The mailbox is private, so the questions and the emails are not published. The method, the scripts
and every number came from one run on 29 September 2026.

## How results were graded

To score a search, you need the right answers to each question. For every question, the top
documents from every technique were collected into one pool. Claude Opus 5.5 then graded every
question and document pair: 2 if the document answers the question, 1 if it is related, 0 if it is
not. It graded 11,138 pairs.

A grader that is wrong makes every number wrong, so the grader was checked three ways. It graded the
email each question was written from as 2 for 119 of 119 questions. It graded randomly chosen emails
as 0 in 237 of 237 cases. Asked again about 1,480 pairs, it gave the same grade 96.8% of the time. A
person did not check the grades.

One problem was found and fixed during the study. On topic questions the grader leaned too much on
the single example answer it was shown. Its instructions were changed to say that the example is one
of several right answers, and every topic pair was graded again.

| Choice | Why | What we did not do |
|---|---|---|
| A language model grades each pair | It reads the document the way a person would, so it can tell an answer from a mention | Labels from keyword rules. They reward keyword search by construction |
| The strongest available model grades | The grades decide every number in the study | A smaller model, to save cost |
| Every technique's top documents are pooled | A right answer that only one technique found still gets a grade | Grading only the email the question came from. That misses the other right answers |

## The scores, and how to read a difference

Each score looks at the top 10 or top 5 results of one search for one question, and is averaged over
the questions. nDCG@10 is the main score.

| Score | What it measures | Range |
|---|---|---|
| **nDCG@10** | Adds up the grades of the top 10 results, counting results near the top more, and divides by the best possible total for that question | 0 to 1. 1 means the best documents in the best order |
| **recall@10** | The share of all documents graded 2 that appear in the top 10 | 0 to 1 |
| **MRR** | Mean reciprocal rank: 1 divided by the position of the first good result | 0 to 1. 1 means a good result came first |
| **Success@5** | The share of questions with at least one good result in the top 5 | 0 to 1 |
| **pass rate** | For the agent: the share of answers graded correct, complete and supported by the documents the agent read | 0 to 1 |

With 119 questions, two techniques can score differently by chance. So every comparison has a **95%
interval**. It comes from drawing the questions again at random 2,000 times, with repeats allowed,
and measuring the difference each time. If the whole interval is above zero, the technique helped.
If the interval crosses zero, the study could not tell the two apart, and this page says the
difference is **inside the noise**.

## What was compared

Each index was its own database file with one `inillucent_search` table, and each one differed from
the production index in one thing. Every index was searched with every strategy: keyword search
alone, vector search alone, hybrid search with each of three fusion rules, each of those followed by
a reranker, query rewriting and HyDE. A document's rank was the rank of its best chunk.

```mermaid
flowchart LR
    A["Sample 15,000 documents"] --> B["Write 119 questions"]
    B --> C["Split into dev and test halves"]
    C --> D["Build one index for each setup"]
    D --> E["Run every search strategy"]
    E --> F["Pool the top documents"]
    F --> G["Grade each pair 0, 1 or 2"]
    G --> H["Score, and compare with 95% intervals"]
```

| Index | Chunks | Embedding model | What it tests |
|---|---|---|---|
| Production | 900 characters, 120 overlap, heading | Nomic v2, served by a separate program | The starting point |
| Built in model | The same chunks | `nomic-embed-text-v1.5` inside inillucent | Does the switch lose quality |
| No heading | 900 characters, no heading | `nomic-embed-text-v1.5` | What the heading is worth |
| Small chunks | 450 characters, heading | `nomic-embed-text-v1.5` | Smaller chunks |
| Large chunks | 2,000 characters, heading | `nomic-embed-text-v1.5` | Larger chunks |
| Whole messages | 6,000 characters, heading | `nomic-embed-text-v1.5` | One chunk for most emails |
| Summaries | The built in model's chunks, each with a one sentence summary of its document | `nomic-embed-text-v1.5` | Whether document context helps |

## Every comparison

Each row compares one technique with the setup it changes, on the 15,000 document sample. A change
in **bold** has an interval that does not cross zero.

| Technique | Compared with | Change in nDCG@10 | 95% interval |
|---|---|---|---|
| Reranker `bge-reranker-v2-m3` over the top 60 | hybrid search | **+0.106** | +0.070 to +0.147 |
| Reranker `gte-reranker-modernbert-base` over the top 60 | hybrid search | **+0.101** | +0.066 to +0.141 |
| The two rerankers against each other | | +0.004 | -0.017 to +0.026 |
| Heading removed, with a reranker | heading kept | **-0.111** | -0.148 to -0.073 |
| Heading removed, hybrid search | heading kept | **-0.043** | -0.083 to -0.002 |
| Fixed weight 0.5 | adaptive weight | **+0.040** | +0.012 to +0.068 |
| RRF | adaptive weight | **+0.035** | +0.006 to +0.066 |
| Hybrid search | keyword search alone | **+0.038** | +0.021 to +0.058 |
| Hybrid search | vector search alone | +0.005 | -0.042 to +0.057 |
| Document summary on every chunk, hybrid search | no summary | **+0.063** | +0.024 to +0.104 |
| Document summary on every chunk, with a reranker | no summary | +0.023 | -0.006 to +0.055 |
| `nomic-embed-text-v1.5` inside inillucent | Nomic v2, hybrid search | -0.002 | -0.017 to +0.014 |
| 450 character chunks | 900 | -0.014 | -0.045 to +0.014 |
| 2,000 character chunks | 900 | -0.009 | -0.033 to +0.015 |
| 6,000 character chunks | 900 | -0.003 | -0.031 to +0.026 |
| 6,000 character chunks, with a reranker | 900, with a reranker | +0.019 | -0.010 to +0.052 |
| Query rewriting | hybrid search | +0.002 | -0.031 to +0.034 |
| HyDE | hybrid search | +0.005 | -0.034 to +0.046 |

On the whole mailbox of 67,369 emails:

| Setup | nDCG@10 | recall@10 | MRR | Success@5 | Median search time |
|---|---|---|---|---|---|
| Keyword search alone | 0.591 | 0.657 | 0.576 | 0.689 | 24 ms |
| Hybrid search, adaptive weight | 0.635 | 0.730 | 0.616 | 0.748 | 89 ms |
| Hybrid search, RRF | 0.667 | 0.752 | 0.656 | 0.798 | 104 ms |
| Hybrid search, fixed weight 0.5 | 0.672 | 0.771 | 0.657 | 0.798 | 95 ms |
| RRF, then `bge-reranker-v2-m3` over the top 60 | **0.758** | **0.809** | **0.787** | 0.857 | 296 ms |
| Adaptive weight, then `bge-reranker-v2-m3` over the top 60 | 0.749 | 0.794 | **0.788** | **0.866** | 305 ms |
| Adaptive weight, then `gte-reranker-modernbert-base` over the top 60 | 0.755 | 0.808 | 0.775 | **0.866** | 229 ms |

All rows use `nomic-embed-text-v1.5`. The reranker times are on an RTX 5090 graphics card.

## Result: rerank the top candidates

An embedding model reads the question and each chunk separately. A reranker reads the two together
and scores how well the chunk answers the question. Reranking the top 60 chunks with
`bge-reranker-v2-m3` raised nDCG@10 by 0.106 over hybrid search on the sample, with an interval of
+0.070 to +0.147. The smaller `gte-reranker-modernbert-base` raised it by 0.101, and the two
rerankers were tied with each other. On the whole mailbox, hybrid search scored 0.635 and reranking
over RRF scored 0.758. The gain was largest for keyword style questions, vague recollections and
answers inside attachments.

We tried reranking the top 30, 60 and 100 chunks. On the dev half, 30 scored 0.802, 60 scored 0.832
and 100 scored 0.836. 30 lost part of the gain. 100 added nothing measurable and took longer.

A reranker costs time at every search, and on a processor it costs a lot. The median time to rerank
one set of candidates, with 8 threads on the processor:

| Reranker | 20 candidates, graphics card | 60 candidates, graphics card | 20 candidates, processor | 60 candidates, processor |
|---|---|---|---|---|
| `gte-reranker-modernbert-base` | 0.05 s | 0.14 s | 3.1 s | 10.6 s |
| `bge-reranker-v2-m3` | 0.06 s | 0.23 s | 6.5 s | 22.8 s |

**What we decided.** inillucent now has a reranker. `rerank(question, passage)` scores one pair in
SQL, and a `question` column on `inillucent_search` reranks the top `rerank_depth` rows of the fused
list. It runs `gte-reranker-modernbert-base`, which scored the same as `bge-reranker-v2-m3` and is
faster. `rerank_depth` is 60 by default, because 30 lost part of the gain and 100 added nothing. The
reranker is a separate download of about 600 MB and runs only when a query names `question`, because
a program that never reranks should not carry it, and because on a processor it takes seconds.
[Embeddings](../embeddings.md#reranking) has the install command and the measured speeds.

## Result: keep the heading on every chunk

Each chunk began with its email's subject, sender and date. Removing that heading cost 0.043
nDCG@10 with hybrid search, with an interval of -0.083 to -0.002, and 0.111 with a reranker, with an
interval of -0.148 to -0.073.

The loss is larger with a reranker because the reranker reads only the chunk. A chunk that does not
say which email it came from gives the reranker less to judge by.

**What we decided.** inillucent now has `chunk_text(text, size, overlap, heading)`, a table function
that cuts a document into windows and writes the heading at the top of each one. Putting the heading
in the function call makes the setup that won the easiest one to write.

## Result: how to combine keyword and vector search

Hybrid search beat keyword search alone by 0.038 nDCG@10, with an interval of +0.021 to +0.058.
Against vector search alone the gain was +0.005, inside the noise, but hybrid search also keeps the
exact name and number matches that vector search misses.

How the two lists are combined mattered. Against inillucent's default adaptive weight, a fixed
weight of 0.5 gained 0.040 and RRF gained 0.035. On the whole mailbox the adaptive weight scored
0.635, RRF 0.667 and a fixed 0.5 scored 0.672.

inillucent's own score card uses a different collection, with questions about document titles.
There the adaptive weight won: nDCG@10 0.982, against 0.917 for RRF and 0.955 for a fixed 0.5 (see
`inillucent-scorecard.md` in the repository root). With a reranker the rule mattered much less. On
the whole mailbox, reranking over RRF scored 0.758 and reranking over the adaptive weight scored
0.749, inside the noise.

**What we decided.** `inillucent_search` now takes a `fusion` option: `adaptive`, `rrf` or
`weighted`. We did not change the default from `adaptive`. The two collections disagreed, and a new
default would change the results of every table that already exists. The documentation tells you to
try `rrf` and `adaptive` on your own questions.

## Result: chunk size did not matter

Against chunks of 900 characters, chunks of 450 scored -0.014, chunks of 2,000 scored -0.009 and
chunks of 6,000 scored -0.003. With a reranker, 6,000 scored +0.019. Every one of these intervals
crosses zero.

Long chunks have a cost that the scores do not show. The embedding model reads at most 1,900 tokens,
about 7,000 characters of English, and ignores the rest without a message. 19.5% of the 6,000
character chunks were cut.

**What we decided.** We did not build a chunk size into the engine. The documentation recommends 900
characters with an overlap of 100. We built `embed_tokens(text)`, which counts the tokens in a text,
and `inillucent embed` reports how many rows it cut at the limit, so a cut chunk is never silent.

## Result: the built in embedding model is good enough

We compared `nomic-embed-text-v1.5`, which inillucent runs inside the database, with Nomic v2, a
newer model served by a separate program. On the sample the difference was -0.002 nDCG@10, with an
interval of -0.017 to +0.014. On the whole mailbox it was +0.004, with an interval of -0.011 to
+0.019. There was no measurable difference. Hybrid searches were faster with the built in model, 72
ms at the median against 96 ms, because there is no call to another program.

The cost of embedding was the real problem. `embed()` in SQL embeds about 13 chunks a second on the
processor, so the whole mailbox would have taken about 12 hours. The same model on a graphics card
took 22 minutes and gave the same vectors, with a cosine similarity of 0.9999994 between the two.

**What we decided.** We kept `nomic-embed-text-v1.5` and built `inillucent embed`, a command that
embeds every row of a column whose vector is `NULL`, on the processor or on a graphics card with
`--device cuda:0`. A run that stops continues where it ended. We also added
`INILLUCENT_EMBED_THREADS`, because two processes embedding at once slowed each other down.
[Embeddings](../embeddings.md#embedding-a-whole-table) describes the command.

## Result: asking a language model to help the search

Query rewriting asks a language model for three other versions of the question and searches for
each. It changed nDCG@10 by +0.002, with an interval of -0.031 to +0.034. HyDE asks the model for an
imagined answer and searches for chunks near it. It changed nDCG@10 by +0.005, with an interval of
-0.034 to +0.046. Both are inside the noise.

Document summaries ask the model for a one sentence summary of each document and add it to each of
the document's chunks. They added 0.063 to hybrid search, with an interval of +0.024 to +0.104. With
a reranker they added 0.023, inside the noise. Writing a summary for every email in the mailbox
would take about 12 hours of the local model's time.

**What we decided.** We built none of these into inillucent. Each adds a language model call to
every search or to every document, and none of them helped once a reranker ran.

## Result: what the agent's answers showed

Search scores measure the search, and a user sees the answer. So we gave a local language model two
tools, `search_email` and `read_document`, and asked it every question twice with each setup. Claude
Opus 5.5 graded each answer: whether it was correct, whether it was complete, whether every claim was
backed by a document the agent had read, and whether it cited the right document.

On the dev half the pass rate rose from 0.658 to 0.767 over three rounds: the built in embedding
model, then reranking, then a better instruction prompt written after reading the failed answers. On
the test half, the full new setup scored 0.780 against 0.771 for the old one. That change of +0.008
has an interval of -0.068 to +0.085, so it is inside the noise.

The agent made 0.49 fewer tool calls per answer with the new setup. A capable agent makes up for
weaker search by searching again and reading more documents. So better search showed up as less work
for the agent, and it did not show up as more correct answers.

**What we decided.** We report the retrieval gains as retrieval gains and do not claim that they
make an agent more accurate. This is also the reason for the test half: on the dev half alone, the
new setup looked like a gain of 0.108. The better prompt was kept, because it costs nothing and cuts
tool calls.

## The hill climb, round by round

After the first set of comparisons, we changed one setting at a time, scored it on the dev half, and
kept it only when it beat the best setup so far by more than the noise. The test column was never
used to choose. It shows which gains held on questions the choice never saw.

| Round | Change | Dev nDCG@10 | Test nDCG@10 | Kept |
|---|---|---|---|---|
| Start | Production: Nomic v2 and the built in hybrid search | 0.723 | 0.724 | Starting point |
| 1 | Switch to `nomic-embed-text-v1.5` inside inillucent | 0.701 | 0.742 | No, but the climb continued on it, since it is the model inillucent runs |
| 2 | Fixed weight 0.5 in place of the adaptive weight | 0.756 | 0.767 | Yes |
| 3 | RRF in place of the fixed weight | 0.734 | 0.779 | No |
| 4 | Rerank the top 60 of the built in hybrid with `bge-reranker-v2-m3` | 0.832 | 0.822 | Yes |
| 5 | Rerank the top 60 of RRF instead | 0.841 | 0.828 | No, inside the noise |
| 6 | Rerank only the top 30 | 0.802 | 0.800 | No |
| 7 | Rerank the top 100 | 0.836 | 0.819 | No |
| 8 | The smaller `gte-reranker-modernbert-base` | 0.837 | 0.826 | No, tied |
| 9 | 2,000 character chunks under the reranker | 0.840 | 0.851 | No |
| 10 | 6,000 character chunks under the reranker | 0.861 | 0.835 | Yes, inside the noise |
| 11 | A document summary on every chunk | 0.848 | 0.871 | No |

Round 10 was kept by that rule, but its gain was inside the noise, and 2,000 character chunks scored
higher on the test half. So we took no chunk size lesson from it.

## What we built, and what we did not

Every feature below exists because of a result on this page. Each row links to that result.

| Decision | Why |
|---|---|
| Built: `rerank()` and the `question` column of `inillucent_search`, with `rerank_depth` 60 | [The largest gain measured](#result-rerank-the-top-candidates), on every collection |
| Built: the `fusion` option, with the value `rrf` | [RRF and a fixed weight beat the adaptive weight](#result-how-to-combine-keyword-and-vector-search) on the emails |
| Not changed: the default fusion stays `adaptive` | The score card collection found the opposite |
| Built: `chunk_text(text, size, overlap, heading)` | [Removing the heading cost up to 0.111](#result-keep-the-heading-on-every-chunk) |
| Built: `inillucent embed`, on the processor or a graphics card | [22 minutes against about 12 hours](#result-the-built-in-embedding-model-is-good-enough) |
| Built: `embed_tokens()` and a count of cut rows | [19.5% of 6,000 character chunks were cut without a message](#result-chunk-size-did-not-matter) |
| Built: `INILLUCENT_EMBED_THREADS` | Two processes embedding at once slowed each other down |
| Not built: query rewriting, HyDE, document summaries | [No gain once a reranker ran](#result-asking-a-language-model-to-help-the-search) |
| Not built: a default chunk size in the engine | [Size made no measurable difference](#result-chunk-size-did-not-matter) |

## Limits of the study

The same study on a different collection may find different sizes of effect. The two largest
effects here, the reranker and the heading, agree with published work on other collections. The
small effects, such as chunk size, are the ones most likely to differ on other data.

- It used one collection, a private mailbox, so nobody else can run it on the same data.
- A language model graded every pair. It passed three checks, and a person did not check its grades.
- 119 questions is enough to see the large effects. For most comparisons, a difference smaller than
  about 0.03 fell inside the noise.
- A small or quantized reranker over fewer candidates on a processor was not measured. It is the
  next thing to measure for machines with no graphics card.
