# marspeak

中文火星文转换库。把正常中文转换成那种繁体、形近字、生僻异体混着写的火星文，并能还原回来。

```
原文   今天天气真好，我们一起去看看外面的世界
轻度   今天天気眞恏，莪们一起去看看外面的世琾
中度   今靝靝気眞恏，莪们一起去栞栞外面的世琾
重度   今靝靝気眞恏，ωǒ们一起去栞栞外面菂卋琾
还原   今天天气真好，我们一起去看看外面的世界
```

## 特性

- **严格可逆**：默认配置下 `decode(encode(x)) == x`，对任意输入成立。
- **三档强度 + 连续火星化程度**：`Level::{Light, Medium, Heavy}` 控制候选范围，`intensity` 控制多大比例的字会被替换。
- **确定性随机**：相同 `seed` 与输入必定得到相同输出，可以在测试里断言。
- **局部稳定**：新增或删改一个字，只影响那一个位置的字形。位置哈希只统计汉字，插入 emoji、URL、英文都不会影响汉字部分的转换结果。
- **编译期静态表**：字形表用 `phf` 编译成零开销的完美哈希表，无 IO、无初始化。
- **表的一致性在编译期校验**：反向表冲突、候选重复、ASCII 歧义、链式歧义会直接让构建失败。
- **可选 `no_std`**。

## 使用

```toml
[dependencies]
marspeak = "0.1"
```

想用尚未发布的主分支版本：

```toml
marspeak = { git = "https://github.com/Natane0808/marspeak", branch = "main" }
```

```rust
use marspeak::{Level, Marspeak};

let mp = Marspeak::builder()
    .level(Level::Medium)
    .intensity(0.8)
    .seed(42)
    .build()?;

let encoded = mp.encode("今天天气真好");
let decoded = mp.decode(&encoded)?;
assert_eq!(decoded, "今天天气真好");
```

`no_std`：

```toml
marspeak = { version = "0.1", default-features = false }
```

## oneway 候选：为什么不是所有字形都能还原

字形表里有一部分候选本身就是中文日常用字——`甚`、`否`、`冇`、`佔` 这类。它们当火星文很好看，但如果允许还原，一段**从未 encode 过的正常中文**会被改坏：

| 输入 | 若强制还原会得到 | 后果 |
|---|---|---|
| 甚至让我怀疑 | 什至让我怀疑 | 造词错误 |
| 他否认了这件事 | 他不认了这件事 | 语义改变 |
| 是否愿意过来 | 是不愿意过来 | 语义反转 |
| 冇问题的（粤语） | 有问题的 | 语义反转 |

所以这类候选被标记为 **oneway**：默认既不参与 encode 也不参与 decode，`decode` 遇到它们一律保持原样，正常文本永远安全，往返也严格可逆。

想要更浓的火星味，可以显式打开：

```rust
let mp = Marspeak::builder()
    .level(Level::Heavy)
    .allow_ambiguous(true)
    .build()?;
// 此时 encode 会生成甚/否/冇 等字形，但这些字形无法再被 decode 还原
```

原则很简单：**宁可不还原，也不能把用户的正常文本改成反义。**

## API

| 方法 | 说明 |
|---|---|
| `Builder::level(Level)` | 候选范围；默认 `Light` |
| `Builder::intensity(f32)` | 0.0~1.0 的火星化比例；默认 0.6，超出范围返回 `MarspeakError::InvalidIntensity` |
| `Builder::seed(u64)` | 随机种子，保证结果可复现；默认 0 |
| `Builder::allow_ambiguous(bool)` | 是否启用 oneway 候选；默认 false |
| `Marspeak::encode(&str) -> String` | 转火星文，不会失败 |
| `Marspeak::decode(&str) -> Result<String, MarspeakError>` | 还原；字形表出现循环映射时返回 `NonConvergent` |

## 扩展字形表

数据在 `data/glyph.tsv`，每行四个字段，用空格分隔：

```
原字  候选(逗号分隔)  档位   是否 oneway（可选）
我    莪              basic
我    沃,娥           variant oneway
```

- 档位：`basic` / `variant` / `rare`，累计生效（Medium = basic + variant，Heavy = 全部）
- 第四列写 `oneway` 表示该行候选只参与 encode，不参与还原

改完表直接 `cargo build`，`build.rs` 会重新生成静态表，并在编译期挡住以下问题：

- 反向表多对一冲突（一个候选被两个字占用）
- 同一候选重复登记
- 候选本身也是原字（会造成链式歧义）
- 候选以 ASCII 开头（无法与原文中的数字字母区分）
- oneway 候选同时被登记为可逆候选

## 性能

3600 字（约 10.5 KB）长文本，`criterion` 实测于本机：

| 操作 | 耗时 | 吞吐 |
|---|---|---|
| encode 长文本 · Light | 157 µs | 69 MiB/s |
| encode 长文本 · Heavy | 173 µs | 63 MiB/s |
| encode 短句（18 字） | 0.79~0.94 µs | 58~69 MiB/s |
| decode 长文本 | 209~217 µs | ~50 MiB/s |
| decode 短句 | 1.21~1.29 µs | ~43 MiB/s |
| 往返 | 392 µs | 28 MiB/s |

每字符大约 44 ns。跑一下即可看到本机数据，并与上一次结果自动对比：

```bash
cargo bench          # 报告在 target/criterion/report/index.html
```

## 项目结构

```
src/lib.rs        Marspeak / Builder / Level / encode / decode
src/transform.rs  候选挑选与位置哈希
src/table.rs      build.rs 生成的静态表
src/error.rs      MarspeakError
build.rs          TSV -> phf 静态表 + 一致性校验
data/glyph.tsv    字形映射表
benches/glyph.rs  criterion 基准
```

## 路线

当前只实现了**字形替换**一类变换。火星文另外三类——Unicode 装饰（𝕥𝕙𝕚𝕤）、谐音俚语（神马、酱紫）、缩写数字（bhys、520）——属于有损变换，需要分词和消歧，尚未加入。

## License

MIT © 2026 natane

Copyright 署名以 [`LICENSE`](./LICENSE) 文件为准。
