# marspeak

中文火星文转换库：**可逆的**单字符字形替换。

一个字符进、一个字符出，不依赖上下文，所以 `decode(encode(s)) == s` 恒成立。

- **零依赖** —— 没有 `phf`、没有 `build.rs`，映射表在编译期嵌进二进制
- **可逆** —— 不记录操作历史，靠映射表自身的三条规则保证唯一解
- **确定性** —— 同一组 `(seed, intensity, table)` 永远得到同一份输出
- **强度可调** —— `intensity` 控制大约多少比例的字符被替换

## 安装

```toml
[dependencies]
marspeak = "0.2"
```

## 快速开始

```rust
use marspeak::Builder;

// 默认：seed = 0，intensity = 1.0，内置表
let m = Builder::new().build().unwrap();

assert_eq!(m.encode("我爱"), "莪愛");
assert_eq!(m.decode("莪愛"), "我爱");
```

调节强度与种子：

```rust
use marspeak::Builder;

let text = "我今天去了那家新开的面馆，碗里的牛肉炖得很烂。";
let m = Builder::new().seed(42).intensity(0.5).build().unwrap();

let encoded = m.encode(text);

// 不管道过多少强度，decode 都能还原
assert_eq!(m.decode(&encoded), text);
```

## 三个参数

| 参数 | 类型 | 默认 | 含义 |
|---|---|---|---|
| `seed` | `u64` | `0` | 随机种子。决定多候选的字挑中哪一个：同 seed 输出恒定，不同 seed 输出不同 |
| `intensity` | `f32` | `1.0` | 替换强度，`0.0..=1.0`。约为「百分之多少的**有映射**字符被替换」，具体哪些由哈希决定 |
| `table` | `&Table` | 内置表 | 映射表。用 `Table::parse` 从 TSV 载入自定义的 |

`intensity` 越界（含 `NaN`）时 `build()` 返回 `MarspeakError::InvalidIntensity`，其他参数不会失败。

## 需要注意的三个语义

**1. 强度是概率，不是"前 N 个字"。** 具体哪些字被换由哈希决定，不是从头数。表里没有的字符 —— ASCII、emoji、空白、标点 —— 无论强度多少都原样通过。

**2. `decode` 是无条件的。** 它会还原输入里**所有**能反查的字符，跟 encode 时用了多少 intensity 无关。所以拿一段别人写的火星文来 decode 同样会生效：

```rust
use marspeak::Builder;

let m = Builder::new().build().unwrap();
// 不是自己 encode 出来的也能还原
assert_eq!(m.decode("莪愛沵"), "我爱你");
```

这是有意的设计后果，不是 bug —— 否则就得在输出里携带元信息，那就不叫"单字符替换"了。

**3. 字符数不变。** 严格一进一出，`encode` 前后的 `chars().count()` 相同，但字节数可能变大（`'a'` → 3 字节汉字）。

## 自定义映射表

TSV 格式，每行 `原字<Tab>候选[,候选...]`：

```
# 注释以 # 开头，空行跳过
我	莪,沃
爱	愛
```

```rust
use marspeak::{Builder, Table};

let table = Table::parse("我\t莪,沃\n爱\t愛\n").unwrap();
let m = Builder::new().table(&table).build().unwrap();

assert_eq!(m.decode(&m.encode("我爱")), "我爱");
```

解析时会校验 6 条规则，违反任何一条都返回 `TableError`，并带上出错的行号：

| # | 规则 | 违反的后果 |
|---|---|---|
| 1 | 每行必须有 Tab 分隔出的第二列 | 结构不对 |
| 2 | 原字和每个候选必须是**恰好一个** `char` | 无法做单字符替换 |
| 3 | 候选 ≠ 原字 | 映射到自身等于什么都没做，还会破坏反查 |
| 4 | 候选全局唯一 | 反查有歧义 |
| 5 | 同一原字不重复出现 | 一个原字多个候选请写在同一行 |
| 6 | 候选不能同时是任何原字 | `encode` 后 `decode` 会套娃 |

**第 3~5 条是可逆性的核心**，坏一条就会出现无法还原的字符。

格式上表达不出来的字符（逗号是分隔符、行尾空格会被 trim）会被规则 2 挡掉 —— 这是格式限制，不是策略限制。除此之外**不挑字符种类**：ASCII、emoji、假名、注音都可以当候选。

## 内置表

3262 条映射、3523 个候选，约 8% 的原字有多个候选（30 KB，编译期嵌入，运行时零 I/O）。
首次用到时才解析，之后由 `OnceLock` 缓存，零成本。

## 实现要点

- 查询走**有序 `Vec` + `binary_search`**，不是 `HashMap` —— 建好之后只读不写，省掉哈希开销
- 哈希是手写的 **FNV-1a 64 位**
- 挑选候选时先把高 32 位异或进低 32 位再取模 —— FNV-1a 低位雪崩性差，不折叠的话同一个字会反复选中同一个候选

## 许可

MIT
