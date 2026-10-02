# A search table whose merges dropped its facets

`app.rdb` was written by the published 2.0.3 Windows binary, `inillucent-shell.exe`, running
`build.sql`. `build.sql` creates an `inillucent_search` table with one facet and `compact = 1`, then
makes 30 commits of 6 rows each. Every third row has `src = 'slack'`, so the table holds 60 of them.

`compact = 1` merges the table's segments after every commit. In 2.0.3 a merge rebuilt each row
without its facet values, so a search with `src = 'slack'` found 2 of the 60 rows, while
`src IN ('slack')` found all 60. After building, the file was checkpointed with 2.0.3's
`inillucent checkpoint`, so `app.rdb` holds every commit and the log segment holds nothing.

`search_facet_compare::a_file_whose_merges_dropped_facets_answers_every_row` opens a copy of this
file with the current build and checks that the search finds all 60 rows, before and after another
commit and after a compaction.

| File | sha256 |
|---|---|
| `app.rdb` | `fc8eacdb7a8711d97a286dccfcccd1da09daa21330763cf89c3aa30a57785b39` |
| `app.rdb-wal.0000000003` | `98b666d48d8ce2bc1874a9aa90adab2445aef19ac5ddefaec076094872284f37` |

The first copy of these two files was never committed, so they were built again on 2026-10-01 from
the published `inillucent-2.0.3-x86_64-pc-windows-msvc.zip`, whose sha256 matched the release's
`SHA256SUMS`. That binary answered `SELECT rowid FROM s WHERE s MATCH 'apple' AND src = 'slack' AND
k = 100000` with 2 rows and the same search with `src IN ('slack')` with 60, on a copy of the checkpointed file.
