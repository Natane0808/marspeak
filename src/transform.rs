//! 字形替换的挑选逻辑。
//!
//! 关键点：是否替换、替换成哪一个候选，都由 `hash(seed, 字符序号, 原字)` 决定，
//! 而不是顺序抽样随机数。这样增删一个字只会影响那一个字，其余位置的火星文保持不变。

use phf::Map;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a：自己实现而不用 `DefaultHasher`，保证跨 Rust 版本、跨平台结果稳定。
///
/// `ordinal` 是该字在输入中的「第几个汉字」，非汉字不占位。
pub(crate) fn mix(seed: u64, ordinal: u64, ch: char) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in seed
        .to_le_bytes()
        .into_iter()
        .chain(ordinal.to_le_bytes())
        .chain((ch as u32).to_le_bytes())
    {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// 是否为 CJK 汉字（含扩展 A）。
///
/// 用于计算「第几个汉字」：标点和拉丁字母不占位，这样往句子里插 emoji 或 URL
/// 不会改变已有汉字的替换结果。
pub(crate) fn is_han(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{2A6DF}'
    )
}

/// 按强度决定是否替换该字，命中则返回选中的候选字形。
///
/// `threshold` 为 0~100 的百分比阈值。是否替换与替换成哪一个候选都由同一个
/// hash 派生，避免每个字符算两遍 FNV。
pub(crate) fn pick<'a>(
    table: &'a Map<&'static str, &'static [&'static str]>,
    ch: char,
    seed: u64,
    ordinal: u64,
    threshold: u64,
) -> Option<&'a str> {
    if threshold == 0 {
        return None;
    }
    let mut buf = [0u8; 4];
    let candidates = *table.get(ch.encode_utf8(&mut buf))?;
    if candidates.is_empty() {
        return None;
    }

    let hash = mix(seed, ordinal, ch);
    if hash % 100 >= threshold {
        return None;
    }
    // 低位用于判定是否替换，这里混合高低位派生挑选下标，避免第二次哈希
    let choice = (hash ^ (hash >> 32)) % candidates.len() as u64;
    Some(candidates[choice as usize])
}
