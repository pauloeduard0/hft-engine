use std::sync::atomic::AtomicU64;

pub static TRADE_ERRORS: AtomicU64 = AtomicU64::new(0);
