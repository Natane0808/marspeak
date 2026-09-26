use std::{
    error::Error,
    fs::{self},
    io::{self, IsTerminal, Read},
    path::PathBuf,
};

use clap::{Parser, Subcommand};
use marspeak::{Builder, Table};

fn main() {
    let cli = Cli::parse();

    // 位置参数优先；只有没给位置参数时才去读 stdin。
    // buffer 必须活到 convert 之后，所以声明在 match 外面。
    let mut buffer = String::new();
    let text = match &cli.mode {
        Mode::Encode { text, .. } | Mode::Decode { text, .. } => text,
    };

    // 两个开关分开算，因为它们的判断条件不同：
    // interactive 还要求位置参数为空（否则不算"手敲输入"这个场景）。
    // 注意：提示一律走 stderr —— 打进 stdout 会破坏重定向出来的文件。
    let interactive = text.is_none() && io::stdin().is_terminal();
    let on_terminal = io::stdout().is_terminal();

    if interactive {
        eprintln!("等待输入，结束时按 Ctrl+Z 回车（Git Bash / Unix 用 Ctrl+D）：");
    }

    let result = match text {
        Some(_) => convert(&cli, &buffer),
        None => {
            // 用 read_to_string 而不是 read_line：后者一次只读一行，
            // 管道喂进来的多行文本会被静默截断。
            if let Err(e) = io::stdin().read_to_string(&mut buffer) {
                eprintln!("读取输入时出错：{e}");
                std::process::exit(1);
            }
            convert(&cli, &buffer)
        }
    };

    match result {
        Err(e) => {
            eprintln!("错误：{e}");
            std::process::exit(1);
        }
        Ok(out) => {
            // 手敲输入时，输入和输出会上下紧挨着、长得又像，必须插一行标题分开，
            // 否则根本分不清哪几行是自己敲的、哪几行是结果。
            // 只在这个分支打：给了位置参数就没有这个歧义，不必多一行噪音。
            if interactive && on_terminal {
                eprintln!("——— 结果 ———");
            }

            // 用 print! 而不是 println!：stdin 读进来的文本自带换行，
            // 再补一个就会多出一个空行。只有位置参数这种不带换行的才补。
            print!("{out}");
            if !out.ends_with('\n') {
                println!();
            }

            // 摘要同样只在人看着终端时才打，且走 stderr：
            // 打到 stdout 会破坏 `marspeak-cli encode < a.txt > b.txt` 这类用法。
            if on_terminal {
                let src = text.as_deref().unwrap_or(&buffer);
                eprintln!("—— {}", summarize(&cli, src, &out));
            }
        }
    }
}

/// 生成一行结果摘要，告诉用户到底发生了什么。
///
/// 抽成纯函数是为了能测：它不碰 I/O，也不关心 stdout 是不是终端。
fn summarize(cli: &Cli, src: &str, out: &str) -> String {
    let total = src.chars().count();
    if total == 0 {
        return "输入为空，没有可转换的内容".to_string();
    }

    // 单字符替换，两边字符数必然相同，可以直接 zip
    let changed = src.chars().zip(out.chars()).filter(|(a, b)| a != b).count();

    match &cli.mode {
        Mode::Encode {
            seed, intensity, ..
        } => {
            if changed == 0 {
                format!("{total} 个字符，一个都没换：intensity 为 0，或这些字在表里没有映射")
            } else {
                format!("{total} 个字符，替换了 {changed} 个（seed {seed}，intensity {intensity}）")
            }
        }
        Mode::Decode { .. } => {
            if changed == 0 {
                format!("{total} 个字符，没有可还原的（表里没有这些字的候选）")
            } else {
                format!("{total} 个字符，还原了 {changed} 个")
            }
        }
    }
}

/// 中文火星文转换命令行工具：可逆的单字符字形替换。
///
/// 文本可以走位置参数，也可以走 stdin（二选一，位置参数优先）。
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand, Debug)]
enum Mode {
    /// 把正常文本转换成火星文
    Encode {
        /// 随机种子。决定多候选的字挑中哪一个：同 seed 输出恒定，不同 seed 输出不同
        #[arg(long, default_value_t = 0)]
        seed: u64,

        /// 替换强度，0.0 ~ 1.0。约为「百分之多少的有映射字符被替换」
        #[arg(long, default_value_t = 1.0_f32)]
        intensity: f32,

        /// 自定义映射表（TSV）。不给则用内置表
        #[arg(long)]
        table: Option<PathBuf>,

        /// 待转换文本。不给则读 stdin
        text: Option<String>,
    },

    /// 把火星文还原成正常文本
    Decode {
        /// 自定义映射表（TSV）。必须与 encode 时用的是同一张，否则还原结果无意义
        #[arg(long)]
        table: Option<PathBuf>,

        /// 待还原文本。不给则读 stdin
        text: Option<String>,
    },
}

/// 执行一次转换。抽成纯函数是为了能测：`input` 由调用方喂进来，
/// 所以测试里不必真的去碰 stdin。
fn convert(cli: &Cli, input: &str) -> Result<String, Box<dyn Error>> {
    let (tbl_path, text) = match &cli.mode {
        Mode::Encode { table, text, .. } | Mode::Decode { table, text } => (table, text),
    };

    // 位置参数优先，没有才退回 stdin 读进来的内容
    let src = text.as_deref().unwrap_or(input);

    // owned 的 Table 必须先被一个局部变量持有，才能从它借出 &Table。
    // 写成 `Some(p) => &Table::parse(...)?` 会是经典的
    // "temporary value dropped while borrowed"。
    let custom = match tbl_path {
        Some(path) => Some(Table::parse(&fs::read_to_string(path)?)?),
        None => None,
    };

    // Table::builtin() 是 &'static，会自动协变到和 Some(t) 那一臂相同的生命周期
    let tbl = match &custom {
        Some(table) => table,
        None => Table::builtin(),
    };

    match &cli.mode {
        Mode::Encode {
            seed, intensity, ..
        } => {
            let marspeak = Builder::new()
                .seed(*seed)
                .intensity(*intensity)
                .table(tbl)
                .build()?;
            Ok(marspeak.encode(src))
        }
        // decode 不需要 seed / intensity：还原是确定性的，
        // 唯一性由映射表的三条规则（候选≠原字、候选唯一、候选不能是原字）保证
        Mode::Decode { .. } => {
            let marspeak = Builder::new().table(tbl).build()?;
            Ok(marspeak.decode(src))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use super::{Cli, Mode, convert, summarize};

    /// 写一张小表到临时目录。每个用例用不同文件名，避免 cargo test 并行时互相踩。
    fn temp_table(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("marspeak-cli-test-{name}.tsv"));
        std::fs::write(&path, content).expect("写临时表失败");
        path
    }

    /// 用 try_parse_from 而不是 parse_from：后者遇到参数错误或 --help
    /// 会直接 process::exit，把测试进程一起干掉。
    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut argv = vec!["marspeak-cli"];
        argv.extend_from_slice(args);
        Cli::try_parse_from(argv)
    }

    #[test]
    fn encode_defaults_are_seed_zero_and_full_intensity() {
        let cli = parse(&["encode", "你好"]).expect("合法参数应解析成功");
        let Mode::Encode {
            seed,
            intensity,
            table,
            text,
        } = &cli.mode
        else {
            panic!("应解析出 Encode，实际是 {:?}", cli.mode);
        };
        assert_eq!(*seed, 0);
        assert_eq!(*intensity, 1.0);
        assert!(table.is_none());
        assert_eq!(text.as_deref(), Some("你好"));
    }

    #[test]
    fn decode_rejects_seed_and_intensity() {
        // struct variant 里没有这两个字段，clap 会自动拒绝。
        // 这条是「decode 不接受随机参数」的守卫：静默忽略会让用户以为参数生效了。
        assert!(parse(&["decode", "--seed", "1", "莪"]).is_err());
        assert!(parse(&["decode", "--intensity", "0.5", "莪"]).is_err());
        assert!(parse(&["decode", "莪"]).is_ok());
    }

    #[test]
    fn missing_subcommand_is_a_usage_error() {
        assert!(parse(&[]).is_err(), "缺子命令必须报错");
        assert!(parse(&["convert", "x"]).is_err(), "未知子命令必须报错");
        assert!(parse(&["encode", "--nope", "x"]).is_err(), "未知选项必须报错");
    }

    #[test]
    fn convert_round_trips_through_a_custom_table() {
        // 用自定义小表而不是内置表：内置表内容变了这条测试也不该红
        let path = temp_table("roundtrip", "我\t莪\n爱\t愛\n");
        let cli = parse(&[
            "encode",
            "--table",
            path.to_str().expect("临时路径应为合法 UTF-8"),
            "我爱",
        ])
        .unwrap();
        let encoded = convert(&cli, "").unwrap();
        assert_eq!(encoded, "莪愛");

        let cli = parse(&[
            "decode",
            "--table",
            path.to_str().expect("临时路径应为合法 UTF-8"),
            &encoded,
        ])
        .unwrap();
        assert_eq!(convert(&cli, "").unwrap(), "我爱");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn custom_table_does_not_leak_builtin_entries() {
        // 自定义表里只有「我」。内置表里有「爱→愛」，但既然指定了自定义表，
        // 内置表就不该再参与，否则 decode 会得到无法预测的结果。
        let path = temp_table("override", "我\t莪\n");
        let cli = parse(&[
            "encode",
            "--table",
            path.to_str().expect("临时路径应为合法 UTF-8"),
            "爱",
        ])
        .unwrap();
        assert_eq!(convert(&cli, "").unwrap(), "爱");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn convert_reports_invalid_intensity() {
        // 校验交给 Builder::build()，CLI 只负责把错误冒泡出来。
        // 这里断言的是消息走 Display（"invalid intensity: 1.5"），不是 Debug。
        let cli = parse(&["encode", "--intensity", "1.5", "我爱"]).unwrap();
        let err = convert(&cli, "").unwrap_err();
        assert!(
            err.to_string().contains("invalid intensity"),
            "错误消息应可读，实际是 {err}"
        );
    }

    #[test]
    fn convert_reports_missing_table_file() {
        let cli = parse(&["encode", "--table", "nonexistent-table.tsv", "我爱"]).unwrap();
        assert!(convert(&cli, "").is_err(), "表文件不存在必须报错而不是崩溃");
    }

    #[test]
    fn convert_prefers_positional_text_over_stdin_input() {
        // main 传进来的 input 是 stdin 的内容；有位置参数时必须忽略它
        let cli = parse(&["encode", "--intensity", "0", "我爱"]).unwrap();
        assert_eq!(convert(&cli, "不该被用上的 stdin 内容").unwrap(), "我爱");
    }

    #[test]
    fn convert_falls_back_to_stdin_input() {
        // 没给位置参数时才用 input
        let cli = parse(&["encode", "--intensity", "0"]).unwrap();
        assert_eq!(convert(&cli, "我爱").unwrap(), "我爱");
    }

    #[test]
    fn summarize_reports_replaced_count() {
        // 摘要得说清「换了几个 / 一共几个 / 用的什么参数」，
        // 否则用户看到一堆看不懂的字，不知道是成功了还是参数没生效
        let cli = parse(&["encode", "--seed", "3", "--intensity", "1", "我爱"]).unwrap();
        let out = convert(&cli, "").unwrap();
        let s = summarize(&cli, "我爱", &out);
        assert!(s.contains("2 个字符"), "应报字符总数，实际 {s}");
        assert!(s.contains("替换了 2 个"), "应报替换数，实际 {s}");
        assert!(s.contains("seed 3"), "应带上 seed，实际 {s}");
    }

    #[test]
    fn summarize_explains_when_nothing_changed() {
        // 干巴巴报个「替换了 0 个」会被当成 bug，得给出可能的原因
        let cli = parse(&["encode", "--intensity", "0", "我爱"]).unwrap();
        let s = summarize(&cli, "我爱", "我爱");
        assert!(s.contains("一个都没换"), "应解释为什么没换，实际 {s}");
    }

    #[test]
    fn summarize_for_decode() {
        let cli = parse(&["decode", "莪愛"]).unwrap();
        let out = convert(&cli, "").unwrap();
        let s = summarize(&cli, "莪愛", &out);
        assert!(s.contains("还原了 2 个"), "decode 摘要应报还原数，实际 {s}");
    }

    #[test]
    fn summarize_handles_empty_input() {
        let cli = parse(&["encode", ""]).unwrap();
        assert!(
            summarize(&cli, "", "").contains("输入为空"),
            "空输入要单独说明，否则用户不知道是失败还是本来就没内容"
        );
    }

    #[test]
    fn seed_changes_the_output() {
        let path = temp_table("seed", "我\t莪,沃\n");
        let arg = path.to_str().expect("临时路径应为合法 UTF-8");
        let text = "我我我我我我我我";

        let outs: Vec<String> = ["0", "1", "2", "3"]
            .iter()
            .map(|seed| {
                let cli = parse(&["encode", "--seed", seed, "--table", arg, text]).unwrap();
                convert(&cli, "").unwrap()
            })
            .collect();

        let distinct = {
            let mut sorted = outs.clone();
            sorted.sort();
            sorted.dedup();
            sorted.len()
        };
        assert!(
            distinct >= 2,
            "不同 seed 应产生不同输出，实际 {outs:?} 只有 {distinct} 种"
        );

        let _ = std::fs::remove_file(path);
    }
}
