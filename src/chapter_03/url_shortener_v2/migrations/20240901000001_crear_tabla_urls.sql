CREATE TABLE IF NOT EXISTS urls (
    code        VARCHAR(10)  PRIMARY KEY,
    target_url  TEXT         NOT NULL,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ,
    clicks      BIGINT       NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_urls_expires
    ON urls (expires_at)
    WHERE expires_at IS NOT NULL;
