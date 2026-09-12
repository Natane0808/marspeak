use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

const DATA_FILE: &str = "data/glyph.tsv";
const TIERS: [&str; 3] = ["basic", "variant", "rare"];
/// 标记为 oneway 的列值：候选只用于 encode，不进反向表。
const ONEWAY: &str = "oneway";

fn main() {
    println!("cargo:rerun-if-changed={DATA_FILE}");

    let raw = std::fs::read_to_string(DATA_FILE)
        .unwrap_or_else(|e| panic!("无法读取字形表 {DATA_FILE}: {e}"));
    // 去掉可能存在的 UTF-8 BOM，否则首行原字会被判为多字符
    let source = raw.trim_start_matches('\u{feff}');

    // 档位 -> 原字 -> 候选列表。
    // all_tiers 含全部候选；safe_tiers 只含可逆候选（即未标记 oneway 的）。
    let mut all_tiers: BTreeMap<&str, BTreeMap<char, Vec<String>>> = BTreeMap::new();
    let mut safe_tiers: BTreeMap<&str, BTreeMap<char, Vec<String>>> = BTreeMap::new();
    for tier in TIERS {
        all_tiers.insert(tier, BTreeMap::new());
        safe_tiers.insert(tier, BTreeMap::new());
    }
    // 反向表：候选 -> 原字（必须多对一），只收录可逆候选
    let mut reverse: BTreeMap<String, char> = BTreeMap::new();
    let mut oneway_candidates: BTreeSet<String> = BTreeSet::new();

    for (lineno, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let no = lineno + 1;
        let mut fields = line.split_whitespace();
        let key = fields.next().unwrap_or_else(|| panic!("第 {no} 行缺少原字: {line}"));
        let cands = fields.next().unwrap_or_else(|| panic!("第 {no} 行缺少候选: {line}"));
        let tier = fields.next().unwrap_or_else(|| panic!("第 {no} 行缺少档位: {line}"));
        let oneway = match fields.next() {
            Some(ONEWAY) => true,
            Some(other) => panic!("第 {no} 行未知第四列 `{other}`（只支持 {ONEWAY}）: {line}"),
            None => false,
        };

        let mut chars = key.chars();
        let key_char = chars
            .next()
            .unwrap_or_else(|| panic!("第 {no} 行原字为空: {line}"));
        assert!(
            chars.next().is_none(),
            "第 {no} 行原字必须是单个字符: {line}"
        );
        assert!(
            TIERS.contains(&tier),
            "第 {no} 行档位非法（应为 basic/variant/rare）: {tier}"
        );

        let list: Vec<String> = cands
            .split(',')
            .map(|c| c.trim())
            .filter(|c| !c.is_empty())
            .map(str::to_string)
            .collect();
        assert!(!list.is_empty(), "第 {no} 行候选为空: {line}");

        for candidate in &list {
            // 以 ASCII 开头的候选（如数字、字母谐音）decode 时会与原文中的数字字母混淆，一律拒绝
            let first = candidate.chars().next().expect("候选非空");
            assert!(
                !first.is_ascii(),
                "第 {no} 行候选 `{candidate}` 以 ASCII 字符开头，decode 时无法与原文区分"
            );
            if oneway {
                oneway_candidates.insert(candidate.clone());
                continue;
            }
            match reverse.get(candidate) {
                Some(&owner) if owner == key_char => {
                    panic!("第 {no} 行候选 `{candidate}` 在 `{key_char}` 名下重复出现")
                }
                Some(&owner) => panic!(
                    "反向表冲突：候选 `{candidate}` 同时指向 `{owner}` 与 `{key_char}`，decode 将无法还原"
                ),
                None => {
                    reverse.insert(candidate.clone(), key_char);
                }
            }
        }

        all_tiers
            .get_mut(tier)
            .expect("tier 已初始化")
            .entry(key_char)
            .or_default()
            .extend(list.iter().cloned());
        if !oneway {
            safe_tiers
                .get_mut(tier)
                .expect("tier 已初始化")
                .entry(key_char)
                .or_default()
                .extend(list);
        }
    }

    // oneway 候选若同时是某个可逆候选，encode 生成它之后会被 decode 还原成别的字，
    // 属于静默错误，必须在编译期挡住。
    for candidate in &oneway_candidates {
        assert!(
            !reverse.contains_key(candidate),
            "oneway 候选 `{candidate}` 与其他可逆候选重复，decode 会把它还原成错误的字"
        );
    }

    // 候选不得是任何原字，否则 decode 会形成链式歧义
    let origins: BTreeSet<char> = all_tiers.values().flat_map(|m| m.keys().copied()).collect();
    for (candidate, owner) in &reverse {
        let mut it = candidate.chars();
        if let (Some(c), None) = (it.next(), it.next()) {
            assert!(
                !origins.contains(&c),
                "候选 `{candidate}`（属于 `{owner}`）本身也是原字，会造成 decode 链式歧义"
            );
        }
    }

    // 多字符候选（如 `ωǒ`）的首字符必须独一无二，不能自己也是某个候选，
    // 否则 decode 的「单字符优先」快路径会误判。
    let mut multi_first: BTreeSet<char> = BTreeSet::new();
    for candidate in reverse.keys() {
        if candidate.chars().count() > 1 {
            let first = candidate.chars().next().expect("候选非空");
            let mut buf = [0u8; 4];
            assert!(
                !reverse.contains_key(first.encode_utf8(&mut buf)),
                "多字符候选 `{candidate}` 的首字符 `{first}` 本身也是候选，单字符优先匹配会出错"
            );
            multi_first.insert(first);
        }
    }

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR 未设置");
    let out_path = Path::new(&out_dir).join("glyph_tables.rs");
    let mut writer = BufWriter::new(File::create(&out_path).expect("无法创建生成文件"));

    // 累计表：Light 只含 basic，Medium 含 basic+variant，Heavy 含全部
    write_accumulated(&mut writer, &safe_tiers, "", "可逆候选表");
    write_accumulated(&mut writer, &all_tiers, "_AMBIGUOUS", "含 oneway 歧义候选的表");

    write_reverse_map(&mut writer, &reverse);
    write_multi_first_set(&mut writer, &multi_first);

    let max_chars = reverse
        .keys()
        .map(|candidate| candidate.chars().count())
        .max()
        .unwrap_or(1);
    writeln!(
        writer,
        "/// 反向表中最长候选的字符数，decode 时据此做最长匹配。\npub const MAX_CANDIDATE_CHARS: usize = {max_chars};"
    )
    .expect("写入常量失败");

    writer.flush().expect("写入生成文件失败");
}

/// 生成三张累计候选表，名字为 `GLYPH_<LEVEL>{suffix}`。
fn write_accumulated<W: Write>(
    w: &mut W,
    tiers: &BTreeMap<&str, BTreeMap<char, Vec<String>>>,
    suffix: &str,
    comment: &str,
) {
    writeln!(w, "/// {comment}").expect("写入注释失败");
    let names = [
        format!("GLYPH_LIGHT{suffix}"),
        format!("GLYPH_MEDIUM{suffix}"),
        format!("GLYPH_HEAVY{suffix}"),
    ];
    let mut accumulated: BTreeMap<char, Vec<String>> = BTreeMap::new();
    for (idx, tier) in TIERS.iter().enumerate() {
        for (ch, list) in tiers.get(tier).expect("tier 已初始化") {
            accumulated
                .entry(*ch)
                .or_default()
                .extend(list.iter().cloned());
        }
        write_map(w, &names[idx], &accumulated);
    }
}

fn write_map<W: Write>(w: &mut W, name: &str, table: &BTreeMap<char, Vec<String>>) {
    let mut builder = phf_codegen::Map::<&str>::new();
    for (ch, cands) in table {
        let mut buf = [0u8; 4];
        let key: &str = Box::leak(ch.encode_utf8(&mut buf).to_string().into_boxed_str());
        let value = format!(
            "&[{}]",
            cands.iter().map(|c| literal(c)).collect::<Vec<_>>().join(", ")
        );
        builder.entry(key, &value);
    }
    writeln!(
        w,
        "pub static {name}: phf::Map<&'static str, &'static [&'static str]> = {};",
        builder.build()
    )
    .expect("写入 map 失败");
}

fn write_reverse_map<W: Write>(w: &mut W, reverse: &BTreeMap<String, char>) {
    let mut builder = phf_codegen::Map::<&str>::new();
    for (candidate, owner) in reverse {
        let key: &str = Box::leak(candidate.clone().into_boxed_str());
        let mut buf = [0u8; 4];
        let value = literal(owner.encode_utf8(&mut buf));
        builder.entry(key, &value);
    }
    writeln!(
        w,
        "pub static GLYPH_REVERSE: phf::Map<&'static str, &'static str> = {};",
        builder.build()
    )
    .expect("写入反向 map 失败");
}

/// 多字符候选的首字符集合。decode 只有遇到这些字符才展开多字符窗口匹配。
fn write_multi_first_set<W: Write>(w: &mut W, firsts: &BTreeSet<char>) {
    let mut set = phf_codegen::Set::<char>::new();
    for ch in firsts {
        set.entry(*ch);
    }
    writeln!(
        w,
        "pub static GLYPH_MULTI_FIRST: phf::Set<char> = {};",
        set.build()
    )
    .expect("写入首字符集合失败");
}

fn literal(s: &str) -> String {
    format!("\"{}\"", s.escape_debug())
}
