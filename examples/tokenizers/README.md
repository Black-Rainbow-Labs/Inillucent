# Tokenizer packs

A tokenizer pack tells an `inillucent_search` table how to turn the words of one language into
terms. This folder holds one pack, `pt-br.json`, for Brazilian Portuguese. This page shows how to
use it, how it was made, how well it matches PostgreSQL, and how to write a pack for another
language. [Tokenizer packs](../../docs/tokenizer-packs.md) is the reference for the file format.

## Use it

```sh
inillucent tokenizer install --file examples/tokenizers/pt-br.json
inillucent create app.rdb
inillucent --db app.rdb exec "CREATE VIRTUAL TABLE docs USING inillucent_search(body, tokenize = 'pt-br')"
inillucent --db app.rdb exec "INSERT INTO docs(rowid, body) VALUES (1, 'Política de reembolso'), (2, 'Notificação enviada'), (3, 'Horário de atendimento')"
inillucent --db app.rdb query "SELECT rowid, body FROM docs WHERE docs MATCH 'horario'"
```

```text
rowid  body
-----  ----------------------
3      Horário de atendimento
```

The table keeps its own copy of the pack, so it keeps working after
`inillucent tokenizer remove --name pt-br`, and the file opens on a machine that never installed
the pack.

## What is in `pt-br.json`

| Field | Value | Where it comes from |
|---|---|---|
| `stemmer` | `portuguese` | the Snowball Portuguese stemmer, the one PostgreSQL's `portuguese` configuration uses |
| `stopwords` | 203 words | `share/tsearch_data/portuguese.stop` from PostgreSQL 17.5, copied word for word and in order. It includes `tém`, which PostgreSQL's list spells that way |
| `fold` | `true` | accents are folded, so a word typed without them finds the word with them |
| `suffixes` | 12 rules | measured, see below |

The 12 rules put the accent back on an ending that the stemmer reads with its accent:

| A word ending in | is read as ending in |
|---|---|
| `coes`, `cao` | `ções`, `ção` |
| `oes`, `ao` | `ões`, `ão` |
| `ancias`, `ancia` | `âncias`, `ância` |
| `encias`, `encia` | `ências`, `ência` |
| `aveis`, `avel` | `áveis`, `ável` |
| `iveis`, `ivel` | `íveis`, `ível` |

A word is folded first, then a rule rewrites its ending, then it is stemmed, then the stem is folded.
So `notificação`, `notificacao` and `notificações` all become `notific`.

## How well it matches PostgreSQL

The test was whether a word gives the same term as PostgreSQL 17.5 gives it, with the accents of
PostgreSQL's term folded: `unaccent(to_tsvector('portuguese', word))`. The words were the 2,311
words that appear at least twice in the `pt_BR` message catalogs Homebrew installs on macOS. 389 of
them carry an accent. Measured on 2026-10-10 with this build.

| Tokenizer | Words that match, of 2,311 | Accented words that match, of 389 |
|---|---:|---:|
| `pt-br.json` | 2,297 | 376 |
| the same pack with no suffix rules | 2,200 | 278 |

The 14 words that differ:

- 13 accented words end in `-ária`, `-ído`, `-ém` or `-ás`, such as `necessária`, `incluído`,
  `contém` and `aliás`. With the accent folded away these endings read as verb endings
  (`falaria`, `partido`), so the stemmer cuts more. A rule that put the accent back would break the
  verbs, so the pack has none.
- `referencia`, the verb, gives `referent`, because the `encia` rule reads it as the noun
  `referência`. PostgreSQL gives `referenc`.

The Snowball Portuguese stemmer does not join every plural to its singular: `mensagens` and
`mensagem` stay different terms, and so do `canais` and `canal`. A pack cannot change what the
stemmer does inside a word.

## Write a pack for another language

1. Copy `pt-br.json` and change `name`. A name is lowercase letters, digits, `-` and `_`.
2. Set `stemmer` to the language's Snowball stemmer, or to `none`.
3. Put the language's stopwords in `stopwords`. PostgreSQL's `share/tsearch_data/<language>.stop`
   files are a good start. Each word is lowercase letters and digits.
4. Set `fold` to `true` when people type the language without its accents.
5. Add `suffixes` only after measuring them. Run the stemmer over real words with and without the
   rule, and keep the rule only when more words reach the stem you want.
6. Install it with `inillucent tokenizer install --file <pack>.json`. A pack that breaks a rule is
   refused, and the message says which rule.
