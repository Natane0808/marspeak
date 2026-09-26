use std::{error::Error, fmt::Display};

/// 映射表解析失败的原因。每条都对应 [`Table::parse`] 的一条校验规则，
/// 并且都带上出错的行号，方便直接定位到 TSV 的那一行。
///
/// [`Table::parse`]: crate::Table::parse
#[derive(Debug)]
pub enum TableError {
    /// 规则 1：这一行没有 Tab 分隔出的第二列
    MissingColumn {
        /// 出错行号，从 1 开始
        line: usize,
    },
    /// 规则 2：原字和每个候选都必须是恰好一个 `char`
    InvalidField {
        /// 出错行号，从 1 开始
        line: usize,
        /// 出错的字段，`"source"` 或 `"candidate"`
        field: &'static str,
        /// 实际读到的内容
        value: String,
    },
    /// 规则 3：候选 ≠ 原字（映射到自身等于什么都没做，还会破坏反查）
    SelfMapping {
        /// 出错行号，从 1 开始
        line: usize,
        /// 把自己映射成自己的那个字
        ch: char,
    },
    /// 规则 4：候选全局唯一（否则反查有歧义）
    DuplicateCandidate {
        /// 出错行号，从 1 开始
        line: usize,
        /// 重复出现的候选
        candidate: char,
        /// 先占走这个候选的原字
        owner: char,
    },
    /// 规则 5：同一原字不重复出现
    DuplicateSource {
        /// 出错行号，从 1 开始
        line: usize,
        /// 被重复定义的原字
        ch: char,
    },
    /// 规则 6：候选不能同时是任何原字（否则 encode 后 decode 会套娃）
    ChainedCandidate {
        /// 该字作为原字出现的行号，从 1 开始
        line: usize,
        /// 既是别人的候选、又是自己原字的那个字
        candidate: char,
    },
}

impl Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingColumn { line } => write!(
                f,
                "line {line}: missing second column, expected two columns separated by a tab"
            ),
            Self::InvalidField { line, field, value } => write!(
                f,
                "line {line}: {field} must be exactly one character, got {value:?}"
            ),
            Self::SelfMapping { line, ch } => write!(
                f,
                "line {line}: candidate equals its source ({ch:?}), mapping would change nothing"
            ),
            Self::DuplicateCandidate {
                line,
                candidate,
                owner,
            } => write!(
                f,
                "line {line}: candidate {candidate:?} is already claimed by source {owner:?}"
            ),
            Self::DuplicateSource { line, ch } => {
                write!(f, "line {line}: source {ch:?} is defined more than once")
            }
            Self::ChainedCandidate { line, candidate } => write!(
                f,
                "line {line}: candidate {candidate:?} is also a source, forming an ambiguous chain"
            ),
        }
    }
}

impl Error for TableError {}

/// 构造 [`Marspeak`] 时的参数错误。
///
/// [`Marspeak`]: crate::Marspeak
#[derive(Debug)]
pub enum MarspeakError {
    /// intensity 不在 `0.0..=1.0` 范围内（NaN 也属于此类）
    InvalidIntensity(f32),
}

impl Display for MarspeakError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIntensity(intensity) => write!(f, "invalid intensity: {intensity}"),
        }
    }
}

impl Error for MarspeakError {}
