use core::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum MarspeakError {
    /// intensity 超出 0.0~1.0 的合法区间。
    InvalidIntensity(f32),
    /// 反向还原在有限轮次内没有收敛，通常意味着字形表出现了循环映射。
    NonConvergent,
}

impl fmt::Display for MarspeakError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarspeakError::InvalidIntensity(v) => {
                write!(f, "intensity 必须在 0.0~1.0 之间，收到 {v}")
            }
            MarspeakError::NonConvergent => {
                write!(f, "火星文还原未收敛，字形表可能存在循环映射")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MarspeakError {}
