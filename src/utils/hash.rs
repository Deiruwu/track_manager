use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub const FALLBACK_ID_PREFIX: &str = "yt_gen_";

pub fn generate_fallback_id(prefix: &str, raw_data: &str) -> String {
    let mut hasher = DefaultHasher::new();
    raw_data.hash(&mut hasher);
    format!("{}_{:x}", prefix, hasher.finish())
}