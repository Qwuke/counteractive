-- The money store (docs 05 section 4, ARCHITECTURE section 9).
-- No timestamps, no session ids, no amounts: `issued` keeps the request
-- hash and the response, and `spend` keeps sealed refunds only.
-- WITHOUT ROWID with hashed or random keys records no insertion order.

CREATE TABLE issuer (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  seed BLOB NOT NULL,
  key_epoch INTEGER NOT NULL,
  manifest_version INTEGER NOT NULL DEFAULT 0  -- highest full manifest accepted, 0 if none
);

CREATE TABLE issued (
  order_ref BLOB PRIMARY KEY,     -- SHA-256(order_id)
  request_sha256 BLOB NOT NULL,   -- SHA-256 of the issue request bytes
  response BLOB NOT NULL          -- issue_response, not sealed
) WITHOUT ROWID;

CREATE TABLE spend (
  nullifier BLOB PRIMARY KEY,
  sealed_refund BLOB              -- NULL means Pending
) WITHOUT ROWID;
