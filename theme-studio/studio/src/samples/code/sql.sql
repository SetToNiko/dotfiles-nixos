-- Font tournaments: who played whom, and who won.
CREATE TABLE IF NOT EXISTS fonts (
    id          INTEGER PRIMARY KEY,
    family      TEXT    NOT NULL UNIQUE,
    category    TEXT    NOT NULL CHECK (category IN ('monospace', 'sans-serif', 'serif')),
    source      TEXT    NOT NULL DEFAULT 'google',
    size_bytes  INTEGER CHECK (size_bytes > 0),
    added_at    TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS tournaments (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    pool_size   INTEGER NOT NULL CHECK (pool_size BETWEEN 2 AND 128),
    started_at  TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    finished_at TIMESTAMP
);

CREATE TABLE IF NOT EXISTS matches (
    tournament  INTEGER NOT NULL REFERENCES tournaments (id) ON DELETE CASCADE,
    round       INTEGER NOT NULL,
    left_font   INTEGER NOT NULL REFERENCES fonts (id),
    right_font  INTEGER REFERENCES fonts (id),            -- NULL: a bye
    winner      INTEGER REFERENCES fonts (id),
    PRIMARY KEY (tournament, round, left_font)
);

CREATE INDEX IF NOT EXISTS matches_winner ON matches (winner);

INSERT INTO fonts (family, category, size_bytes)
VALUES ('JetBrains Mono', 'monospace', 102400),
       ('Iosevka', 'monospace', 98304),
       ('Inter', 'sans-serif', 131072)
ON CONFLICT (family) DO NOTHING;

-- How far each font got, across every finished tournament.
WITH best AS (
    SELECT m.winner AS font, MAX(m.round) AS round, COUNT(*) AS wins
    FROM matches AS m
    JOIN tournaments AS t ON t.id = m.tournament
    WHERE t.finished_at IS NOT NULL
      AND m.winner IS NOT NULL
    GROUP BY m.winner
)
SELECT f.family,
       f.category,
       COALESCE(b.round, 0)                AS best_round,
       COALESCE(b.wins, 0)                 AS wins,
       ROUND(100.0 * b.wins / SUM(b.wins) OVER (), 1) AS share_pct
FROM fonts AS f
LEFT JOIN best AS b ON b.font = f.id
WHERE f.category = 'monospace'
ORDER BY best_round DESC, wins DESC, f.family
LIMIT 10;
