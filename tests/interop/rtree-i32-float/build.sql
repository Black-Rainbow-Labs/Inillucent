-- Built by inillucent 2.8.1 itself (`tools/cross/bin/releases/2.8.1`). Releases up to 2.8.1 wrote
-- the coordinates of an rtree_i32 table as 32 bit floats, as an rtree table does, where SQLite
-- writes 32 bit integers. 120 rows make the tree split into more than one node. Every number is
-- one a float holds exactly, so what the file means does not depend on a rounded value.
CREATE VIRTUAL TABLE box USING rtree_i32(id, x0, x1, y0, y1);
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 120)
INSERT INTO box SELECT i, i * 1000, i * 1000 + 500, -i * 100, -i * 100 + 50 FROM n;
INSERT INTO box VALUES (1001, 16000000, 16000100, -16000000, -15999900);
INSERT INTO box VALUES (1002, -2000000000, -1999999872, 7, 9);
