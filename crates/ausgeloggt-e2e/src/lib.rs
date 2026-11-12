//! Dev-only support crate: deterministic synthetic mailbox corpus with ground truth,
//! a fake IMAP server, and small helpers shared by the integration tests in `tests/`.

pub mod corpus;
pub mod fake_imap;
pub mod harness;
pub mod rng;
