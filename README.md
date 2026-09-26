# marspeak

中文火星文转换：**可逆的**单字符字形替换。一个字符进、一个字符出，不依赖上下文，所以 `decode(encode(s)) == s` 恒成立。

```
原文   我今天去了那家新开的面馆。
火星文 莪妗兲厾孒那傢噺閞的媔館。
还原   我今天去了那家新开的面馆。
```

## 两个 crate

| crate | 说明 | 依赖 |
|---|---|---|
| [`marspeak`](lib/README.md) | 转换库 | **零依赖** |
| [`marspeak-cli`](cli/README.md) | 命令行工具 | `marspeak` + `clap` |


## 当库用

```toml
[dependencies]
marspeak = "0.2"
```

```rust
use marspeak::Builder;

let m = Builder::new().seed(42).intensity(0.5).build().unwrap();

let text = "我今天去了那家新开的面馆。";
let encoded = m.encode(text);
assert_eq!(m.decode(&encoded), text);
```

## 当命令用

```bash
cargo install marspeak-cli

marspeak-cli encode "我爱你"                  # 莪愛沵
marspeak-cli encode --intensity 0.5 "我爱你"
marspeak-cli decode "莪愛沵"                  # 我爱你
echo "我爱你" | marspeak-cli encode           # 也支持管道
```

## 特性

- **零依赖**（库）—— 映射表在编译期嵌进二进制，没有 `build.rs`，运行时零 I/O
- **可逆** —— 不记录操作历史，靠映射表自身的三条规则保证唯一解
- **确定性** —— 同一组 `(seed, intensity, table)` 永远得到同一份输出
- **强度可调** —— `intensity` 控制大约多少比例的字符被替换

内置 3262 条映射 / 3523 个候选，约 8% 的原字有多个候选。也可以自带 TSV 映射表。

## 需要知道的两个语义

- **强度是概率，不是"前 N 个字"** —— 具体哪些字被换由哈希决定。表里没有的字符（ASCII、emoji、空白、标点）永远原样通过。
- **`decode` 是无条件的** —— 它会还原输入里所有能反查的字符，跟 encode 时用了多少 intensity 无关。拿一段别人写的火星文来 decode 同样会生效，这是有意的设计后果。

完整说明见 [`lib/README.md`](lib/README.md)。

## 目录结构

```
marspeak/
├── lib/     marspeak —— 转换库
│   └── data/marspeak.tsv     内置映射表（编译期嵌入）
└── cli/     marspeak-cli —— 命令行工具
```

## 开发

```bash
cargo test                                        # 两个包一起跑
cargo clippy --workspace --all-targets -- -D warnings
```

## 许可

MIT
