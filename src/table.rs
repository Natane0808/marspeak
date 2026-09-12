//! 由 build.rs 从 `data/glyph.tsv` 生成的编译期静态表。
//!
//! - `GLYPH_LIGHT` / `GLYPH_MEDIUM` / `GLYPH_HEAVY`：累计候选表，档位越高候选越多。
//! - `GLYPH_REVERSE`：反向表，保证多对一，用于 decode。

include!(concat!(env!("OUT_DIR"), "/glyph_tables.rs"));
