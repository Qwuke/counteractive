-- The wallet journal (docs 04 section 4). Created once by `wallet-cli init`.
-- States are stored by name; see `journal::states`.

CREATE TABLE manifest (
  status TEXT PRIMARY KEY CHECK (status IN ('accepted', 'pending')),
  version INTEGER NOT NULL,          -- the accepted version never decreases
  body BLOB NOT NULL,                -- canonical manifest bytes, as signed
  signature BLOB NOT NULL            -- minisign signature text
);

CREATE TABLE purchase (
  purchase_id INTEGER PRIMARY KEY,   -- local; the row exists before the order does
  order_id TEXT UNIQUE,              -- the Checkout Session id, set in AwaitingPayment
  order_secret BLOB NOT NULL,        -- never logged
  product TEXT NOT NULL,             -- product id from the attested release
  credits INTEGER NOT NULL,          -- that product's credits
  issue_state BLOB,                  -- never logged; cleared when collect commits
  issue_request BLOB,                -- sent unchanged on every retry; cleared when collect commits
  state TEXT NOT NULL,               -- Ordered, AwaitingPayment, Collected, Frozen or Abandoned
  chain_id INTEGER                   -- set in the collect transaction
);

CREATE TABLE chain (
  chain_id INTEGER PRIMARY KEY,
  state TEXT NOT NULL,
  credential BLOB,                   -- set in Ready and Dust
  spend_seq INTEGER NOT NULL DEFAULT 0, -- raised in the prove_spend transaction
  spend_state BLOB,                  -- set from Spending on
  spend_message BLOB,
  session_id BLOB,
  budget INTEGER,                    -- s of the current spend
  unknown_turns INTEGER NOT NULL DEFAULT 0 -- turns whose stream died before a receipt
);

CREATE TABLE receipt (
  chain_id INTEGER NOT NULL,
  spend_seq INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  model TEXT NOT NULL,
  prompt_tokens INTEGER NOT NULL,
  completion_tokens INTEGER NOT NULL,
  charge INTEGER NOT NULL,
  remaining INTEGER NOT NULL,
  finish TEXT NOT NULL,
  PRIMARY KEY (chain_id, spend_seq, turn)
);

CREATE TABLE settled_session (
  chain_id INTEGER NOT NULL,
  spend_seq INTEGER NOT NULL,
  s INTEGER NOT NULL,
  t INTEGER NOT NULL,                -- from the refund message itself
  receipts_sum INTEGER NOT NULL,
  outcome TEXT NOT NULL,             -- exact, full_refund, receipt_gap or mismatch
  PRIMARY KEY (chain_id, spend_seq)
);
