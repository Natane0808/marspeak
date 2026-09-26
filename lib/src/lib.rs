//! 中文火星文转换库：**可逆的**单字符字形替换。
//!
//! 一个字符进、一个字符出，不依赖上下文，因此 `decode(encode(s)) == s` 恒成立。
//! 可逆性不靠记录操作历史，而是靠映射表本身的三条规则（见 [`Table`]）：
//! 候选不等于原字、候选全局唯一、候选不能同时是别人的原字。
//!
//! ```
//! use marspeak::Builder;
//!
//! let m = Builder::new().seed(42).intensity(0.6).build().unwrap();
//! let encoded = m.encode("我爱你");
//! assert_eq!(m.decode(&encoded), "我爱你");
//! ```
//!
//! # 两个容易踩的语义
//!
//! - **intensity 是概率，不是"前 N 个字"**。它表示约有百分之多少的「有映射字符」
//!   被替换，具体哪些由哈希决定。表里没有的字符（ASCII、emoji、空白）永远原样通过。
//! - **`decode` 是无条件的**。它会还原输入里所有能反查的字符，跟 encode 时用了
//!   多少 intensity 无关。所以 `decode` 一段别人写的火星文同样会生效——这是有意
//!   的设计后果，不是 bug。
//!
//! # 确定性
//!
//! 同一组 `(seed, intensity, table)` 永远得到同一份输出。哈希是手写的 FNV-1a，
//! 不依赖任何外部状态或平台特性。

mod error;
mod table;

pub use {
    error::{MarspeakError, TableError},
    table::Table,
};

/// 一个配置好的转换器。由 [`Builder`] 产出，可重复使用。
pub struct Marspeak<'a> {
    /// 随机种子，影响多候选字符挑中哪一个
    seed: u64,
    /// intensity 预先换算成的整数百分比 0..=100。
    /// 存整数而不是 f32，是为了让每个字符的判定只做整数比较。
    threshold: u64,
    /// 借用的映射表
    table: &'a Table,
}

impl<'a> Marspeak<'a> {
    /// 把正常文本转换成火星文。不会失败，字符数保持不变。
    pub fn encode(&self, text: &str) -> String {
        // 只按字节数预留：字符数不变但字节数可能变大（'a' → 3 字节汉字），
        // 最坏扩容一次。先跑一遍 chars().count() 反而多一次遍历，不划算。
        let mut result = String::with_capacity(text.len());
        let mut ordinal = 0;

        for ch in text.chars() {
            // 只查一次表：candidates() 是一次 binary_search，属于每个字符都要走的热路径
            let cands = self.table.candidates(ch);
            let cand = pick(cands, ch, self.seed, ordinal, self.threshold);

            // 约定：ordinal 只对「表里有映射的字符」递增，且从 0 开始。
            // 这样在句中插入空格或标点，不会改变后面所有字的替换结果。
            // 顺序固定在 pick 之后——改到前面就变成 off-by-one，golden 值会失配。
            if cands.is_some() {
                ordinal += 1;
            }

            result.push(cand.unwrap_or(ch));
        }

        result
    }

    /// 把火星文还原成正常文本。
    ///
    /// 不需要 seed 也不需要 intensity：还原是查反向表的确定性操作。
    /// 如果哪天这里需要随机参数，说明表规则被破坏了。
    pub fn decode(&self, text: &str) -> String {
        text.chars()
            .map(|ch| self.table.original(ch).unwrap_or(ch))
            .collect()
    }
}

/// [`Marspeak`] 的构造器。默认 seed = 0、intensity = 1.0、用内置表。
pub struct Builder<'a> {
    seed: u64,
    intensity: f32,
    table: &'a Table,
}

impl Builder<'static> {
    /// 以内置表为默认表创建构造器。等价于 `Builder::default()`。
    pub fn new() -> Self {
        Self {
            seed: Default::default(),
            intensity: 1.0,
            table: Table::builtin(),
        }
    }
}

impl<'a> Builder<'a> {
    /// 设置随机种子。同 seed 输出恒定，不同 seed 会改变多候选字符的挑选结果。
    pub fn seed(self, seed: u64) -> Self {
        Self {
            seed,
            intensity: self.intensity,
            table: self.table,
        }
    }

    /// 设置替换强度，必须是 `0.0..=1.0`。
    ///
    /// 语义是「约有百分之多少的有映射字符被替换」。非法值（含 NaN）
    /// 不会在这里报错，而是留给 [`Builder::build`] 统一校验。
    pub fn intensity(self, intensity: f32) -> Self {
        Self {
            seed: self.seed,
            intensity,
            table: self.table,
        }
    }

    /// 换一张映射表。会消耗 self，并返回一个生命周期为 `'b` 的新构造器
    /// ——写成 `-> Self` 会把 `'a` 钉死在旧表上，传自定义表就编译不过了。
    pub fn table<'b>(self, table: &'b Table) -> Builder<'b> {
        Builder {
            seed: self.seed,
            intensity: self.intensity,
            table,
        }
    }

    /// 校验参数并产出 [`Marspeak`]。只有 intensity 可能不合法。
    pub fn build(self) -> Result<Marspeak<'a>, MarspeakError> {
        // 用 contains 而不是 `intensity < 0.0 || intensity > 1.0`：
        // NaN 的两个比较都是 false，那种写法会把 NaN 放过去。
        if !(0.0..=1.0).contains(&self.intensity) {
            return Err(MarspeakError::InvalidIntensity(self.intensity));
        }

        Ok(Marspeak {
            seed: self.seed,
            // round 而不是直接 as：0.29f32 * 100.0 的实际值是 28.999996，
            // 截断会得到 28，用户写 0.29 却拿到 28%，很反直觉。
            threshold: (self.intensity * 100.0).round() as u64,
            table: self.table,
        })
    }
}

impl Default for Builder<'static> {
    fn default() -> Self {
        Self::new()
    }
}

/// 决定一个字符换不换、换成哪个。
///
/// 返回 `None` 表示不替换（调用方把原字符原样写回），
/// `Some(c)` 表示替换成 `c`。
///
/// 四条出口按顺序短路：表里没有 → intensity 为 0 → 哈希落选 → 选中候选。
fn pick(cands: Option<&[char]>, ch: char, seed: u64, ordinal: u64, threshold: u64) -> Option<char> {
    // ① 表里没有这个字：即便 intensity 拉满也不能凭空造字
    let cands = cands?;

    // ② intensity = 0：直通快路径，连哈希都不用算
    if threshold == 0 {
        return None;
    }

    let hash = mix(seed, ordinal, ch);

    // ③ 哈希落选。`hash % 100` 落在 [0, threshold) 才替换，
    //    方向写成 >= 会让 intensity 1.0 变成 0%、0.2 变成 80%。
    if hash % 100 >= threshold {
        return None;
    }

    // ④ 选第几个候选。先把高 32 位异或进低 32 位再取模：
    //    接下来要 % len，而 len 通常只有 1~3，只看最低两三位；
    //    FNV-1a 的低位雪崩性差，不折叠的话同一个字会反复选中同一个候选。
    let idx = (hash ^ (hash >> 32)) % cands.len() as u64;
    Some(cands[idx as usize])
}

/// 把 (seed, ordinal, ch) 打包成 20 字节再哈希。
///
/// 布局：`seed` 小端 8 字节 + `ordinal` 小端 8 字节 + `ch as u32` 小端 4 字节。
/// 这个布局一旦改动，所有已生成的火星文输出都会失效，所以有 golden 测试钉着。
fn mix(seed: u64, ordinal: u64, ch: char) -> u64 {
    let seed_bytes = seed.to_le_bytes();
    let ordinal_bytes = ordinal.to_le_bytes();
    let ch_bytes = (ch as u32).to_le_bytes();
    let mut bytes = [0u8; 20];

    bytes[..8].copy_from_slice(&seed_bytes);
    bytes[8..16].copy_from_slice(&ordinal_bytes);
    bytes[16..20].copy_from_slice(&ch_bytes);

    fnv1a(&bytes)
}

/// FNV-1a 64 位哈希。
///
/// 选它是因为实现只有五行、零依赖、且分布够用。
/// 注意必须 `wrapping_mul`：普通 `*` 在 debug 构建下遇到溢出会 panic。
fn fnv1a(bytes: &[u8]) -> u64 {
    // 起始值（offset basis）
    let offset_basis = 0xcbf29ce484222325_u64;
    // FNV 质数
    let prime = 0x100000001b3_u64;

    let mut hash_value = offset_basis;
    for &byte in bytes {
        hash_value ^= u64::from(byte);
        hash_value = hash_value.wrapping_mul(prime);
    }
    hash_value
}

#[cfg(test)]
mod tests {
    use super::{Builder, Marspeak, Table, fnv1a, mix, pick};

    // 覆盖多种字符类别：汉字、标点、换行、ASCII、emoji、表里没有的字
    const TEXT: &str = "我昨天去了那家新开的面馆，碗里的牛肉炖得很烂，汤头也够鲜。\n\
        老板说他从一九九八年就开始做这行了，中间搬过三次店面。\
        Emoji 🎉 与 ASCII abc 混排。";

    fn encoder(seed: u64, intensity: f32) -> Marspeak<'static> {
        Builder::new()
            .seed(seed)
            .intensity(intensity)
            .build()
            .expect("intensity 在合法范围内")
    }

    #[test]
    fn fnv1a_matches_reference_vectors() {
        // FNV-1a 64 位的标准测试向量。这三条只验证算法本体，
        // 与 mix 的字节布局无关——算法写错和字节拼错要能分开定位。
        assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn mix_matches_golden_values() {
        // 输入布局是 seed(le 8) + ordinal(le 8) + (ch as u32)(le 4) = 20 字节。
        // 这些数字一旦变动，意味着字节序或 ch 的编码方式被改了，
        // 而那会让所有已生成的火星文输出全部失效——所以必须钉死。
        let cases = [
            ((0, 0, '我'), 0x9f17257c2340932au64, 66u64),
            ((0, 1, '我'), 0x4806cde8b33ed8af, 47),
            ((0, 2, '我'), 0x123ac0ba513db460, 12),
            ((1, 0, '我'), 0xda39d5aa93d498cf, 51),
            ((0, 0, 'a'), 0x508acbd48aee9644, 32),
            ((42, 7, '天'), 0xf250f3e4bc8bc18e, 10),
        ];

        for ((seed, ordinal, ch), want_hash, want_pct) in cases {
            let hash = mix(seed, ordinal, ch);
            assert_eq!(
                hash, want_hash,
                "mix({seed}, {ordinal}, {ch:?}) 应为 0x{want_hash:016x}，实际 0x{hash:016x}"
            );
            assert_eq!(
                hash % 100,
                want_pct,
                "mix({seed}, {ordinal}, {ch:?}) 的 %100 用于 intensity 判定"
            );
        }
    }

    #[test]
    fn mix_is_deterministic() {
        // 同输入永远同输出、且不需要任何外部状态。
        // 这是能对具体字符串写断言、以及「跨平台跨版本结果恒定」的前提。
        for ordinal in 0..50u64 {
            assert_eq!(mix(42, ordinal, '我'), mix(42, ordinal, '我'));
        }
        // seed 与 ordinal 各自都要能影响结果，否则挑选会退化
        assert_ne!(mix(0, 0, '我'), mix(1, 0, '我'));
        assert_ne!(mix(0, 0, '我'), mix(0, 1, '我'));
    }

    #[test]
    fn mix_is_well_distributed() {
        // intensity 靠 hash % 100 决定「换不换」，所以这 100 个桶必须覆盖完整，
        // 否则某些强度档位永远换不动某些字。
        let mut buckets = [0u32; 100];
        for ordinal in 0..1000u64 {
            buckets[(mix(0, ordinal, '我') % 100) as usize] += 1;
        }
        let covered = buckets.iter().filter(|&&c| c > 0).count();
        assert_eq!(
            covered, 100,
            "1000 次采样应覆盖全部 100 个桶，实际只覆盖 {covered} 个"
        );
    }

    #[test]
    fn pick_returns_none_when_char_is_absent_from_table() {
        // 表里没有这个字时，即便 intensity 拉满也不能凭空造字
        assert_eq!(pick(None, '我', 0, 0, 100), None);
    }

    #[test]
    fn pick_returns_none_at_zero_intensity() {
        // intensity = 0.0 是直通快路径：不该替换，也不该算哈希
        let cands = ['莪', '沃'];
        assert_eq!(pick(Some(&cands), '我', 0, 0, 0), None);
        assert_eq!(pick(Some(&cands), '我', 42, 7, 0), None);
    }

    #[test]
    fn pick_threshold_boundary() {
        // 用已知的 golden value 做精确边界测试，不依赖统计：
        // mix(0, 0, '我') % 100 == 66，所以 threshold 66 时不换、67 时换。
        // 换的话下标是 (hash ^ hash >> 32) % 3 == 1，即 'B'。
        let cands = ['A', 'B', 'C'];
        assert_eq!(
            pick(Some(&cands), '我', 0, 0, 66),
            None,
            "hash % 100 == 66，threshold 66 时应不替换（66 不小于 66）"
        );
        assert_eq!(
            pick(Some(&cands), '我', 0, 0, 67),
            Some('B'),
            "hash % 100 == 66，threshold 67 时应替换，且下标为 1"
        );
    }

    #[test]
    fn pick_replacement_ratio_matches_threshold() {
        // intensity 的语义就是「约百分之多少的字被替换」。
        // 方向写反（写成 >=）会让 60 变成 40，这条测试能直接抓住。
        let cands = ['A', 'B', 'C'];
        let total = 10_000u64;
        let cases = [(0u64, 0u64), (20, 2000), (60, 6000), (100, 10000)];

        for (threshold, want) in cases {
            let replaced = (0..total)
                .filter(|&ordinal| pick(Some(&cands), '我', 0, ordinal, threshold).is_some())
                .count() as u64;
            let allowed = (want / 20).max(50); // 至少留 5% 或 50 个的余量
            assert!(
                replaced.abs_diff(want) <= allowed,
                "threshold {threshold} 期望替换约 {want} 个，实际 {replaced} 个"
            );
        }
    }

    #[test]
    fn pick_always_returns_a_candidate_and_covers_them_all() {
        // 下标派生若写错（比如忘记异或高位），某些候选可能永远选不中
        let cands = ['A', 'B', 'C'];
        let mut seen = [false; 3];

        for ordinal in 0..1000u64 {
            let got = pick(Some(&cands), '我', 0, ordinal, 100).expect("threshold 100 必然替换");
            let pos = cands
                .iter()
                .position(|&c| c == got)
                .unwrap_or_else(|| panic!("返回了不在候选里的字符 {got:?}"));
            seen[pos] = true;
        }

        assert!(
            seen.iter().all(|&s| s),
            "三个候选都应被选中，实际选中情况 {seen:?}"
        );
    }

    #[test]
    fn pick_is_deterministic() {
        // 同 seed + 同位置 + 同字 = 同结果，这是可逆性的前提
        let cands = ['A', 'B', 'C'];
        for ordinal in 0..100u64 {
            assert_eq!(
                pick(Some(&cands), '我', 42, ordinal, 60),
                pick(Some(&cands), '我', 42, ordinal, 60)
            );
        }
    }

    #[test]
    fn build_rejects_intensity_out_of_range() {
        for intensity in [-0.1, 1.1, 2.0, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(
                Builder::new().intensity(intensity).build().is_err(),
                "intensity {intensity} 超出 [0.0, 1.0]，必须报错"
            );
        }
        // NaN 是最容易漏的一种：若校验写成 `intensity < 0.0 || intensity > 1.0`，
        // NaN 两个比较都是 false，会直接混过去，然后被 `as u64` 转成一个垃圾阈值。
        assert!(
            Builder::new().intensity(f32::NAN).build().is_err(),
            "NaN 必须被拒"
        );
    }

    #[test]
    fn build_accepts_boundary_intensities() {
        assert!(Builder::new().intensity(0.0).build().is_ok());
        assert!(Builder::new().intensity(1.0).build().is_ok());
    }

    #[test]
    fn zero_intensity_is_an_identity_transform() {
        assert_eq!(
            encoder(0, 0.0).encode(TEXT),
            TEXT,
            "intensity = 0.0 时 encode 必须原样返回"
        );
    }

    #[test]
    fn full_intensity_replaces_every_mappable_char() {
        let table = Table::builtin();
        let encoded = encoder(0, 1.0).encode(TEXT);
        let mappable = TEXT
            .chars()
            .filter(|&ch| table.candidates(ch).is_some())
            .count();
        let changed = TEXT
            .chars()
            .zip(encoded.chars())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            changed, mappable,
            "intensity = 1.0 时，表里有映射的 {mappable} 个字符应全部被替换，实际替换了 {changed} 个"
        );
    }

    #[test]
    fn encode_preserves_char_count() {
        for intensity in [0.0, 0.33, 0.7, 1.0] {
            let encoded = encoder(7, intensity).encode(TEXT);
            assert_eq!(
                encoded.chars().count(),
                TEXT.chars().count(),
                "intensity {intensity}：单字符替换不应改变字符数"
            );
        }
    }

    #[test]
    fn round_trip_is_identity() {
        // 这条是全套测试里最重要的一条：它同时验证 ordinal 只影响 encode、
        // reverse 表方向正确、候选全局唯一、边界强度不崩。
        let cases = [
            (0u64, 0.0f32),
            (0, 0.3),
            (0, 1.0),
            (42, 0.5),
            (u64::MAX, 1.0),
            (7, 0.87),
        ];
        for (seed, intensity) in cases {
            let m = encoder(seed, intensity);
            let encoded = m.encode(TEXT);
            assert_eq!(
                m.decode(&encoded),
                TEXT,
                "seed={seed} intensity={intensity} 时 decode(encode(s)) 必须等于 s"
            );
        }
    }

    #[test]
    fn empty_string_round_trips() {
        let m = encoder(0, 1.0);
        assert_eq!(m.encode(""), "");
        assert_eq!(m.decode(""), "");
    }

    #[test]
    fn chars_without_mapping_pass_through() {
        let m = encoder(0, 1.0);
        // 内置表里没有 ASCII、空格、emoji，也没有「的」这个字
        assert_eq!(m.encode("ASCII 123 !@#"), "ASCII 123 !@#");
        assert_eq!(m.encode("的"), "的");
        assert_eq!(m.encode("🎉"), "🎉");
    }

    #[test]
    fn decode_ignores_seed_and_intensity() {
        let low = encoder(0, 0.0);
        let high = encoder(u64::MAX, 1.0);
        assert_eq!(low.decode("莪愛恏"), "我爱好");
        assert_eq!(
            high.decode("莪愛恏"),
            "我爱好",
            "decode 不依赖 seed 与 intensity——若哪天需要，说明设计出了问题"
        );
    }

    #[test]
    fn decode_restores_arbitrary_martian_text() {
        // 不是自己 encode 出来的火星文也能被还原（这是有意的，README 要写明）
        assert_eq!(encoder(0, 0.0).decode("莪們"), "我们");
    }

    #[test]
    fn seed_diversifies_output() {
        let mut outs: Vec<String> = (0..8u64)
            .map(|seed| encoder(seed, 1.0).encode(TEXT))
            .collect();
        outs.sort();
        outs.dedup();
        assert!(
            outs.len() >= 4,
            "8 个不同 seed 应产生至少 4 种不同输出，实际只有 {} 种",
            outs.len()
        );
    }

    #[test]
    fn encode_is_reproducible() {
        assert_eq!(
            encoder(42, 0.6).encode(TEXT),
            encoder(42, 0.6).encode(TEXT),
            "同参数必须得到完全相同的输出"
        );
    }

    #[test]
    fn custom_table_overrides_builtin() {
        let table = Table::parse("我\t莪,沃\n").unwrap();
        let m = Builder::new().table(&table).intensity(1.0).build().unwrap();

        // 「爱」在内置表里有映射（愛），在自定义表里没有 → 必须原样通过
        assert_eq!(m.encode("爱"), "爱", "自定义表生效时内置表不应再参与");

        let out = m.encode("我");
        assert!(
            out == "莪" || out == "沃",
            "替换结果必须来自自定义表，实际 {out:?}"
        );
        assert_eq!(m.decode(&out), "我");
    }

    #[test]
    fn ordinal_starts_at_zero_for_the_first_mapped_char() {
        // mix 的 golden 值是从 ordinal = 0 开始标定的，encode 必须与之对齐。
        // 若第一个有映射的字符用的是 ordinal = 1，golden 值就失去参照意义，
        // 而且输出会随实现细节无声漂移——用户已经生成过的火星文会变。
        let cands = Table::builtin().candidates('你');
        let at_zero =
            pick(cands, '你', 0, 0, 100).expect("内置表里应有「你」，且 intensity = 1.0 必然替换");
        assert_eq!(
            encoder(0, 1.0).encode("你"),
            at_zero.to_string(),
            "第一个有映射的字符应使用 ordinal = 0"
        );
    }

    #[test]
    fn ordinal_only_advances_on_mapped_chars() {
        // 设计约定：ordinal 只对「表里有映射的字符」递增。
        // 否则在文字中间插入一个空格或标点，后面所有字的替换结果都会跟着变。
        // 用「你」而不是「我」：内置表里 我→莪 只有一个候选，换不换都一样，测不出问题。
        let m = encoder(7, 1.0);
        let packed = m.encode("你你你你你你你你");
        let spaced = m.encode("你 你 你 你 你 你 你 你");
        let stripped: String = spaced.chars().filter(|&ch| ch != ' ').collect();
        assert_eq!(
            packed, stripped,
            "插入空格不应改变其他字符的替换结果：\n紧凑 {packed:?}\n带空格 {stripped:?}"
        );
    }
}
