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
| `app.rdb` | `e3bc2f557ecc8e6f9e72a7a2091baa771620e5a09fe1882ede329dbcd1e5c80e` |
| `app.rdb-wal.0000000003` | `57e025de2a0c8dd9ffc51a4cd1833b3dbeb8e9a50eccaed244d8fbfeb1c8e2a9` |
