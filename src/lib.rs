//! `marspeak` —— 火星文转换库。
//!
//! 当前版本只实现「字形替换」这一类变换：把汉字替换成繁体、形近字或生僻异体字。
//! 默认配置下该变换是**多对一且严格可逆**的，`encode` 之后一定能 `decode` 还原。
//! 之所以强调「默认配置」，是因为字形表里有一部分候选（如 `甚`、`否`、`冇`）本身就是
//! 中文日常用字，若允许它们参与还原，正常的中文会被改坏甚至语义反转
//! （「是否愿意」会变成「是不愿意」）。这类候选被标记为 oneway，
//! 只有显式调用 [`Builder::allow_ambiguous`] 时才会参与 encode。
//!
//! ```
//! use marspeak::{Level, Marspeak};
//!
//! let mp = Marspeak::builder()
//!     .level(Level::Medium)
//!     .intensity(0.8)
//!     .seed(42)
//!     .build()
//!     .unwrap();
//!
//! let encoded = mp.encode("今天天气真好");
//! let decoded = mp.decode(&encoded).unwrap();
//! assert_eq!(decoded, "今天天气真好");
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

mod error;
mod table;
mod transform;

pub use error::MarspeakError;

use crate::table::{
    GLYPH_HEAVY, GLYPH_HEAVY_AMBIGUOUS, GLYPH_LIGHT, GLYPH_LIGHT_AMBIGUOUS, GLYPH_MEDIUM,
    GLYPH_MEDIUM_AMBIGUOUS, GLYPH_MULTI_FIRST, GLYPH_REVERSE, MAX_CANDIDATE_CHARS,
};
use crate::transform::{is_han, pick};

/// 还原时的最大迭代轮次，用于防御字形表出现循环映射。
const MAX_ROUNDS: usize = 8;

/// 火星文强度档位，决定候选字形的范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    /// 轻度：只做最常见的替换，基本等价于繁体化。
    #[default]
    Light,
    /// 中度：繁体 + 形近字。
    Medium,
    /// 重度：再加上生僻异体与符号化写法。
    Heavy,
}

/// 已配置好的转换器，可以并发共享（`&Marspeak` 即可）。
#[derive(Debug, Clone)]
pub struct Marspeak {
    seed: u64,
    level: Level,
    /// 火星化百分比（0~100），在建构建时就算好，encode 热路径不碰浮点。
    threshold_pct: u8,
    allow_ambiguous: bool,
}

/// `Marspeak` 的构造器。
#[derive(Debug, Clone)]
pub struct Builder {
    seed: u64,
    level: Level,
    intensity: f32,
    allow_ambiguous: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            seed: 0,
            level: Level::default(),
            intensity: 0.6,
            allow_ambiguous: false,
        }
    }
}

impl Builder {
    /// 随机种子。相同 seed + 相同输入必定得到相同输出。
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// 强度档位，默认 [`Level::Light`]。
    pub fn level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    /// 火星化程度，0.0 表示原样输出，1.0 表示每个能换的字都换。默认 0.6。
    pub fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }

    /// 是否启用「歧义候选」。
    ///
    /// 字形表里有一部分字形（如 `甚`、`否`、`冇`、`佔`）本身就是中文日常用字。
    /// 它们作为火星文很好看，但如果允许它们参与 decode，一段未经 encode 的正常中文
    /// 就会被改坏甚至语义反转（「是否愿意」→「是不愿意」）。
    ///
    /// 因此这类候选被标记为 oneway：默认（false）既不参与 encode 也不参与 decode，
    /// `encode` → `decode` 的往返严格可逆；
    /// 设为 true 则允许 encode 生成它们以加强火星味，代价是这部分字形无法还原。
    pub fn allow_ambiguous(mut self, allow: bool) -> Self {
        self.allow_ambiguous = allow;
        self
    }

    pub fn build(self) -> Result<Marspeak, MarspeakError> {
        if !(0.0..=1.0).contains(&self.intensity) || self.intensity.is_nan() {
            return Err(MarspeakError::InvalidIntensity(self.intensity));
        }
        Ok(Marspeak {
            seed: self.seed,
            level: self.level,
            threshold_pct: (self.intensity * 100.0) as u8,
            allow_ambiguous: self.allow_ambiguous,
        })
    }
}

impl Marspeak {
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// 把正常文本转换成火星文。
    pub fn encode(&self, input: &str) -> String {
        let threshold = self.threshold_pct as u64;
        let table = match (self.level, self.allow_ambiguous) {
            (Level::Light, false) => &GLYPH_LIGHT,
            (Level::Medium, false) => &GLYPH_MEDIUM,
            (Level::Heavy, false) => &GLYPH_HEAVY,
            (Level::Light, true) => &GLYPH_LIGHT_AMBIGUOUS,
            (Level::Medium, true) => &GLYPH_MEDIUM_AMBIGUOUS,
            (Level::Heavy, true) => &GLYPH_HEAVY_AMBIGUOUS,
        };

        let mut out = String::with_capacity(input.len());
        // ordinal 只统计汉字：前缀多一个 emoji、URL 或英文字母，
        // 后面所有汉字的替换结果都保持不变。
        let mut ordinal: u64 = 0;
        for ch in input.chars() {
            match pick(table, ch, self.seed, ordinal, threshold) {
                Some(glyph) => out.push_str(glyph),
                None => out.push(ch),
            }
            if is_han(ch) {
                ordinal += 1;
            }
        }
        out
    }

    /// 把火星文还原成正常文本。
    ///
    /// 字形替换是可逆的，因此正常情况下总能还原；
    /// 若字形表出现循环映射，则返回 [`MarspeakError::NonConvergent`]。
    pub fn decode(&self, input: &str) -> Result<String, MarspeakError> {
        let max_len = MAX_CANDIDATE_CHARS.max(1);
        let mut current = input.to_string();

        for _ in 0..MAX_ROUNDS {
            let mut next = String::with_capacity(current.len());
            let mut changed = false;
            let mut probe = String::with_capacity(8);
            let mut index = 0;

            while index < current.len() {
                let Some(ch) = current[index..].chars().next() else {
                    break;
                };
                let mut buf = [0u8; 4];

                // 快路径：绝大多数候选都是单字符，直接用栈缓冲查表，零分配
                if let Some(original) = GLYPH_REVERSE.get(ch.encode_utf8(&mut buf)) {
                    next.push_str(original);
                    changed = true;
                    index += ch.len_utf8();
                    continue;
                }

                // 慢路径：只有 ωǒ 这类多字符候选的首字符才展开窗口尝试更长匹配
                let mut long_hit = None;
                if max_len > 1 && GLYPH_MULTI_FIRST.contains(&ch) {
                    let mut end = index;
                    let mut count = 0;
                    for next_ch in current[index..].chars() {
                        end += next_ch.len_utf8();
                        count += 1;
                        if count < 2 {
                            continue;
                        }
                        probe.clear();
                        probe.push_str(&current[index..end]);
                        if let Some(original) = GLYPH_REVERSE.get(probe.as_str()) {
                            long_hit = Some((original, end - index));
                            break;
                        }
                        if count >= max_len {
                            break;
                        }
                    }
                }

                match long_hit {
                    Some((original, len)) => {
                        next.push_str(original);
                        changed = true;
                        index += len;
                    }
                    None => {
                        next.push(ch);
                        index += ch.len_utf8();
                    }
                }
            }

            if !changed {
                return Ok(current);
            }
            current = next;
        }
        Err(MarspeakError::NonConvergent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPUS: &[&str] = &[
        "今天天气真好",
        "我们一起去看看外面的世界",
        "学习是一件快乐的事情",
        "你好，我很好，谢谢你的关心",
        "时间过得真快，转眼又是一年",
        "太阳下山了，月亮升起来",
        "这本书写得很好，值得一读",
        "mixed 中英 123 混排 🌍 也不怕",
    ];

    fn converter(level: Level, intensity: f32, seed: u64) -> Marspeak {
        Marspeak::builder()
            .level(level)
            .intensity(intensity)
            .seed(seed)
            .build()
            .unwrap()
    }

    #[test]
    fn round_trip_is_lossless() {
        for level in [Level::Light, Level::Medium, Level::Heavy] {
            for seed in 0..16u64 {
                let mp = converter(level, 1.0, seed);
                for text in CORPUS {
                    let encoded = mp.encode(text);
                    let decoded = mp.decode(&encoded).unwrap();
                    assert_eq!(decoded, *text, "level={level:?} seed={seed} encoded={encoded}");
                }
            }
        }
    }

    #[test]
    fn zero_intensity_is_identity() {
        let mp = converter(Level::Heavy, 0.0, 7);
        for text in CORPUS {
            assert_eq!(mp.encode(text), *text);
        }
    }

    #[test]
    fn full_intensity_replaces_every_known_char() {
        let mp = converter(Level::Light, 1.0, 0);
        // 「们」未收录，应保持原样 —— 未收录的字永远不会被破坏
        assert_eq!(mp.encode("我们"), "莪们");
    }

    #[test]
    fn same_seed_is_reproducible() {
        let a = converter(Level::Heavy, 0.7, 2024);
        let b = converter(Level::Heavy, 0.7, 2024);
        for text in CORPUS {
            assert_eq!(a.encode(text), b.encode(text));
        }
    }

    #[test]
    fn different_seed_gives_different_output() {
        let text: String = CORPUS.join("");
        let a = converter(Level::Heavy, 0.6, 1).encode(&text);
        let b = converter(Level::Heavy, 0.6, 2).encode(&text);
        assert_ne!(a, b);
    }

    #[test]
    fn editing_one_char_only_affects_that_char() {
        let mp = converter(Level::Medium, 0.8, 99);
        let short = mp.encode("今天天气真好");
        let long = mp.encode("今天天气真好啊");
        assert!(
            long.starts_with(&short),
            "追加字符不应改变已有内容：{short} / {long}"
        );
    }

    #[test]
    fn non_han_input_is_untouched() {
        let mp = converter(Level::Heavy, 1.0, 3);
        let text = "Hello, world! 123 🌍 https://example.com";
        assert_eq!(mp.encode(text), text);
    }

    #[test]
    fn invalid_intensity_is_rejected() {
        assert_eq!(
            Marspeak::builder().intensity(1.5).build().unwrap_err(),
            MarspeakError::InvalidIntensity(1.5)
        );
        assert!(Marspeak::builder().intensity(-0.1).build().is_err());
        assert!(Marspeak::builder().intensity(f32::NAN).build().is_err());
    }

    #[test]
    fn empty_input_is_noop() {
        let mp = converter(Level::Heavy, 1.0, 0);
        assert_eq!(mp.encode(""), "");
        assert_eq!(mp.decode("").unwrap(), "");
    }

    /// 一批候选本身就是中文日常用字，被标记为 oneway，
    /// 无论 seed 与档位如何都不能被 decode 还原——否则正常文本会被改坏甚至语义反转。
    #[test]
    fn normal_chinese_survives_decode() {
        const SENTENCES: &[&str] = &[
            "甚至让我怀疑",
            "他否认了这件事",
            "是否愿意过来",
            "冇问题的",
            "芯片的价格上涨",
            "他俯瞰全景",
            "这位郝先生",
            "山西汾酒很有名",
            "几位大佬到场",
            "一次美丽的邂逅",
        ];
        for level in [Level::Light, Level::Medium, Level::Heavy] {
            for seed in 0..8u64 {
                let mp = converter(level, 1.0, seed);
                for text in SENTENCES {
                    assert_eq!(
                        mp.decode(text).unwrap(),
                        *text,
                        "正常文本被改动了：level={level:?} seed={seed}"
                    );
                }
            }
        }
    }

    /// 默认配置下 encode 只使用可逆候选，所以往返严格成立（已由 round_trip 覆盖）。
    /// 开启 allow_ambiguous 后候选池变宽，火星味更足，但不再保证可逆。
    #[test]
    fn allow_ambiguous_widens_encode_pool() {
        let text = "有什么不好呢，大家都看着";
        let differs = (0..64u64).any(|seed| {
            let strict = converter(Level::Heavy, 1.0, seed).encode(text);
            let loose = Marspeak::builder()
                .level(Level::Heavy)
                .intensity(1.0)
                .seed(seed)
                .allow_ambiguous(true)
                .build()
                .unwrap()
                .encode(text);
            strict != loose
        });
        assert!(differs, "开启 allow_ambiguous 后应当能选出更多字形");
    }

    /// 位置哈希使用字符序号：前缀出现 emoji 或英文不影响后续汉字的结果。
    #[test]
    fn ascii_prefix_does_not_shift_results() {
        let mp = converter(Level::Heavy, 1.0, 11);
        let plain = mp.encode("今天天气真好");
        let prefixed = mp.encode("🌍 今天天气真好");
        assert!(prefixed.ends_with(&plain));
    }
}
