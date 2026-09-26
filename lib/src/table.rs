use std::{collections::HashMap, sync::OnceLock};

use crate::error::TableError;

/// 内置映射表在编译期就嵌进二进制，运行时零 I/O。
/// 路径相对于本文件（`lib/src/table.rs` → `lib/data/marspeak.tsv`）。
const BUILTIN_TSV: &str = include_str!("../data/marspeak.tsv");
/// 惰性解析：没用到 Table 就不付解析成本。OnceLock 保证只解析一次。
static BUILTIN: OnceLock<Table> = OnceLock::new();

/// 一张字符映射表。
///
/// 内部是两张**有序**的 `Vec`，查询走 `binary_search`：
///
/// - `forward`：`(原字, 候选列表)`，按原字排序，供 `encode` 用
/// - `reverse`：`(候选, 原字)`，按候选排序，供 `decode` 用
///
/// 有序性是 `binary_search` 的前提，由 [`Table::parse`] 末尾的排序保证；
/// 两张表都必须排序，否则查询结果是未定义的（不是"查不到"，是"查错"）。
///
/// 候选列表用 `Box<[char]>` 而不是 `Vec`：建好之后长度不再变，
/// 去掉容量字段能省一点内存。
#[derive(Debug)]
pub struct Table {
    forward: Vec<(char, Box<[char]>)>,
    reverse: Vec<(char, char)>,
}

impl Table {
    /// 内置表。首次调用时解析并缓存，之后是零成本。
    ///
    /// 只在内置表本身损坏（不可能，除非打包时文件被改坏）时 panic。
    pub fn builtin() -> &'static Table {
        BUILTIN.get_or_init(|| Table::parse(BUILTIN_TSV).expect("内置映射表损坏"))
    }

    /// 查一个字符的候选列表。没有映射则返回 `None`。
    pub fn candidates(&self, ch: char) -> Option<&[char]> {
        // 用 by_key 版本并闭包取 `.0`：写成 `|&(k, _)| k` 会试图把 Box 移动出来，编译不过
        let idx = self.forward.binary_search_by_key(&ch, |k| k.0).ok()?;
        Some(self.forward[idx].1.as_ref())
    }

    /// 反查：这个候选对应哪个原字。decode 的唯一解就来自这里。
    pub fn original(&self, ch: char) -> Option<char> {
        let idx = self.reverse.binary_search_by_key(&ch, |k| k.0).ok()?;
        Some(self.reverse[idx].1)
    }

    /// 解析 TSV 文本。内置表和用户表走的是同一个入口。
    ///
    /// 格式：每行 `原字<Tab>候选[,候选...]`，`#` 开头是注释，空行跳过。
    ///
    /// 校验规则如下，其中第 3~5 条是可逆性的核心：
    /// `decode` 能唯一确定答案全靠它们，坏一条就会出现无法还原的字符。
    ///
    /// 1. 每行必须有一个 Tab 分隔出第二列
    /// 2. 原字和每个候选都必须是**恰好一个** `char`
    /// 3. 候选 ≠ 原字（映射到自身等于什么都没做，还会破坏反查）
    /// 4. 候选全局唯一（否则反查有歧义）
    /// 5. 同一原字不重复出现（一个原字多个候选是合法的，分两行则不是）
    /// 6. 候选不能同时是任何原字（否则 `encode` 后再 `decode` 会套娃）
    pub fn parse(source: &str) -> Result<Self, TableError> {
        // Windows 记事本存出来的 UTF-8 常带 BOM，它会被当成原字的一部分
        let text = source.trim_start_matches('\u{feff}');
        let mut forward = Vec::new();
        // 原字 → 首次出现的行号，第二趟用来查规则 6
        let mut sources: HashMap<char, usize> = HashMap::new();
        // 候选 → 归属的原字，同时充当「候选是否已存在」的判重集合
        let mut owners: HashMap<char, char> = HashMap::new();
        let mut reverse: Vec<(char, char)> = Vec::new();

        for (i, raw) in text.lines().enumerate() {
            let line_no = i + 1;
            // 只去掉空格和 CR。不能用 trim()：Tab 是分隔符，
            // U+3000 之类的全角空格是合法候选，都会被它吃掉。
            let t = raw.trim_matches(|c: char| c == ' ' || c == '\r');
            if t.is_empty() || t.starts_with("#") {
                continue;
            }

            let Some((src, cands)) = t.split_once('\t') else {
                return Err(TableError::MissingColumn { line: line_no });
            };

            let Ok(src) = src.parse::<char>() else {
                return Err(TableError::InvalidField {
                    line: line_no,
                    field: "source",
                    value: src.to_string(),
                });
            };

            // 规则 5：同一原字不重复出现。一个原字有多个候选是合法的
            // （写在同一行用逗号分隔），分两行则不是。
            // 这一句必须在候选循环**外面**——放进去的话，第二个候选就会误报。
            if sources.insert(src, line_no).is_some() {
                return Err(TableError::DuplicateSource {
                    line: line_no,
                    ch: src,
                });
            }

            let mut forward_cands_vec = vec![];
            for cand in cands.split(',') {
                let Ok(cand) = cand.parse::<char>() else {
                    return Err(TableError::InvalidField {
                        line: line_no,
                        field: "candidate",
                        value: cand.to_string(),
                    });
                };

                // 规则 3：候选 ≠ 原字
                if src == cand {
                    return Err(TableError::SelfMapping {
                        line: line_no,
                        ch: src,
                    });
                }

                // 规则 4：候选全局唯一。insert 返回旧值即说明已被别人占过。
                if let Some(owner) = owners.insert(cand, src) {
                    return Err(TableError::DuplicateCandidate {
                        line: line_no,
                        candidate: cand,
                        owner,
                    });
                }

                // 注意方向：reverse 是 (候选, 原字)，写反了 decode 会静默失效
                reverse.push((cand, src));
                forward_cands_vec.push(cand);
            }

            forward.push((src, forward_cands_vec.into_boxed_slice()));
        }

        // 规则 6：候选不能同时是任何原字。必须等所有行读完才能查——
        // 「莪」可能在第 1 行当候选、第 9000 行才当原字。
        // sources 里存着每个原字的行号，所以这里是 O(n) 查表而不是 O(n²) 嵌套。
        for (src, line_no) in sources.iter() {
            if owners.contains_key(src) {
                return Err(TableError::ChainedCandidate {
                    line: *line_no,
                    candidate: *src,
                });
            }
        }

        // 两张表都必须有序，binary_search 的正确性依赖这一点
        forward.sort_unstable_by_key(|e| e.0);
        reverse.sort_unstable_by_key(|e| e.0);
        Ok(Table { forward, reverse })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_field() {
        let cases = [
            // (输入, 期望行号)
            ("我\tωǒ", 1),     // 候选是多字符：这正是「只留单字符」的守卫
            ("ab\t莪", 1),     // 原字是多字符
            ("我\t莪,,沃", 1), // 候选区里有空元素
        ];

        for (input, want_line) in cases {
            let err = Table::parse(input).unwrap_err();
            assert!(
                matches!(err, TableError::InvalidField { line, .. } if line == want_line),
                "input {input:?} 应报 InvalidField(line = {want_line})，实际是 {err}"
            );
        }
    }

    #[test]
    fn test_missing_column() {
        // 整行没有 Tab（这里是空格分隔），才算结构不对
        let err = Table::parse("我 莪").unwrap_err();
        assert!(
            matches!(err, TableError::MissingColumn { line: 1 }),
            "{err}"
        );
    }

    #[test]
    fn test_invalid_field_reports_real_line_number() {
        // 注释行也占行号，所以错的那行是第 2 行
        let err = Table::parse("# 这是一条注释\n我\tωǒ").unwrap_err();
        assert!(
            matches!(err, TableError::InvalidField { line: 2, .. }),
            "{err}"
        );
    }

    #[test]
    fn test_builtin_table_parses() {
        let t = Table::builtin();
        assert_eq!(t.candidates('万'), Some(['萭', '萬'].as_slice()));
        assert_eq!(t.original('萭'), Some('万'));
        assert_eq!(t.original('與'), Some('与'));
    }

    #[test]
    fn test_success_path() {
        let t = Table::parse("万\t萭,萬\n与\t玙").unwrap();

        assert_eq!(t.candidates('万'), Some(['萭', '萬'].as_slice()));
        assert_eq!(t.candidates('与'), Some(['玙'].as_slice()));
        assert_eq!(t.candidates('喵'), None);

        assert_eq!(t.original('萭'), Some('万'));
        assert_eq!(t.original('萬'), Some('万'));
        assert_eq!(t.original('玙'), Some('与'));
    }

    #[test]
    fn test_self_mapping() {
        let err = Table::parse("我\t我").unwrap_err();
        assert!(
            matches!(err, TableError::SelfMapping { line: 1, ch: '我' }),
            "{err}"
        );
    }

    #[test]
    fn test_accepts_any_character_as_candidate() {
        // 规则 4 删除之后，库不再挑字符种类：ASCII、emoji、假名、注音、符号、
        // 其他文字系统都可以当候选。这是「通用单字符可逆替换引擎」的核心守卫，
        // 以后谁把白名单加回来，这条会立刻红。
        for cand in [
            'a', 'Z', '7', '!', '@', // ASCII
            '😀', '✓', '①', // emoji / 符号 / 带圈数字
            'ル', 'と', 'ㄖ', // 片假名 / 平假名 / 注音
            'α', 'ж', // 其他文字系统
            '莪', '\t', // CJK / 制表符（TSV 格式上可表达）
        ] {
            let tsv = format!("我\t{cand}");
            let t =
                Table::parse(&tsv).unwrap_or_else(|e| panic!("候选 {cand:?} 应被接受，实际是 {e}"));
            assert_eq!(
                t.candidates('我'),
                Some([cand].as_slice()),
                "候选 {cand:?} 未登记到正向表"
            );
            assert_eq!(t.original(cand), Some('我'), "候选 {cand:?} 无法还原为原字");
        }
    }

    #[test]
    fn test_delimiters_cannot_be_candidates() {
        // 逗号是候选分隔符；行尾空格会被 trim 吃掉。这两类字符在 TSV 格式里
        // 根本表达不出来，只能由规则 2 报 InvalidField——不是策略限制，是格式限制。
        for input in ["我\t,", "我\t "] {
            let err = Table::parse(input).unwrap_err();
            assert!(
                matches!(
                    err,
                    TableError::InvalidField {
                        field: "candidate",
                        ..
                    }
                ),
                "input {input:?} 应报 InvalidField(candidate)，实际是 {err}"
            );
        }
    }

    #[test]
    fn test_duplicate_candidate() {
        let err = Table::parse("我\t莪\n尔\t莪").unwrap_err();
        assert!(
            matches!(
                err,
                TableError::DuplicateCandidate {
                    line: 2,
                    candidate: '莪',
                    owner: '我'
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn test_duplicate_source() {
        // 一个原字有多个候选是合法的；被拒的是「同一个原字出现两次」
        let err = Table::parse("我\t莪\n我\t沃").unwrap_err();
        assert!(
            matches!(err, TableError::DuplicateSource { line: 2, ch: '我' }),
            "{err}"
        );
    }

    #[test]
    fn test_chained_candidate() {
        // 莪 是「我」的候选，却又作为原字出现：encode(我)=莪，decode 会把它还原成 我
        let err = Table::parse("我\t莪\n莪\t沃").unwrap_err();
        assert!(
            matches!(
                err,
                TableError::ChainedCandidate {
                    line: 2,
                    candidate: '莪'
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn test_bom_and_crlf_are_tolerated() {
        let t = Table::parse("\u{feff}我\t莪\r\n尔\t尓\r\n").unwrap();
        assert_eq!(t.candidates('我'), Some(['莪'].as_slice()));
        assert_eq!(t.original('尓'), Some('尔'));
    }

    #[test]
    fn test_comments_and_blank_lines_are_skipped() {
        let t = Table::parse("# 注释\n\n我\t莪\n\n# 又一条注释\n尔\t尓").unwrap();
        assert_eq!(t.candidates('我'), Some(['莪'].as_slice()));
        assert_eq!(t.candidates('尔'), Some(['尓'].as_slice()));
    }

    #[test]
    fn test_builtin_table_invariants() {
        let t = Table::builtin();

        // 两张表必须有序，否则 binary_search 的结果是未定义的
        assert!(
            t.forward.windows(2).all(|w| w[0].0 <= w[1].0),
            "forward 未排序"
        );
        assert!(
            t.reverse.windows(2).all(|w| w[0].0 <= w[1].0),
            "reverse 未排序"
        );

        let mut cand_count = 0;
        for (src, cands) in &t.forward {
            assert!(!cands.is_empty(), "原字 {src:?} 没有候选");
            for cand in cands.iter() {
                cand_count += 1;
                assert_ne!(cand, src, "原字 {src:?} 映射到自身");
                assert_eq!(
                    t.original(*cand),
                    Some(*src),
                    "候选 {cand:?} 无法还原为 {src:?}"
                );
                assert!(
                    t.candidates(*cand).is_none(),
                    "候选 {cand:?} 同时也是原字，形成歧义链"
                );
            }
        }

        // 反向表条数必须等于候选总数（一一对应，decode 才有唯一解）
        assert_eq!(t.reverse.len(), cand_count);
    }
}
