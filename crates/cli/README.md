# marspeak-cli

[`marspeak`](https://crates.io/crates/marspeak) 的命令行工具：把文本转成可逆的中文火星文，或还原回来。

## 安装

```bash
cargo install marspeak-cli
```

## 用法

```
marspeak-cli encode|decode [OPTIONS] [TEXT]
```

文本可以走位置参数，也可以走 stdin —— **二选一，位置参数优先**。

```bash
# 位置参数
$ marspeak-cli encode "我今天去了那家新开的面馆。"
莪妗兲厾孒那家噺閞的媔館。

# 强度 0.5：大约一半的字被换
$ marspeak-cli encode --intensity 0.5 "我今天去了那家新开的面馆。"
我今兲去孒那家新閞的媔館。

# 换种子：多候选的字会挑中不同的那个
$ marspeak-cli encode --seed 7 "我今天去了那家新开的面馆。"
莪妗兲厾孒那傢噺開的媔館。

# 还原
$ marspeak-cli decode "莪妗兲厾孒那傢噺閞的媔館。"
我今天去了那家新开的面馆。
```

管道：

```bash
$ echo "我爱你" | marspeak-cli encode
莪愛沵

$ cat novel.txt | marspeak-cli encode --intensity 0.3 > martian.txt
```

### 交互式输入

不带文本直接跑，程序会等你从键盘输入：

```bash
$ marspeak-cli encode
等待输入，结束时按 Ctrl+Z 回车（Git Bash / Unix 用 Ctrl+D）：
我爱你
你好吗
^Z
——— 结果 ———
莪愛沵
妳恏嗎
—— 10 个字符，替换了 6 个（seed 0，intensity 1）
```

结束输入的按键：

| 终端 | 操作 |
|---|---|
| cmd.exe / PowerShell | `Ctrl+Z` 然后 `Enter` |
| Git Bash / Linux / macOS | `Ctrl+D` |

`Ctrl+Z` 要单独在一行开头按，后面还得再敲一次 `Enter`。

结果前后各有一行提示，因为它们都是打给你的、不是转换结果的一部分：

- `——— 结果 ———` 插在输入和输出之间。手敲输入时两边上下紧挨着、长得又像，没有这条线根本分不清哪几行是自己敲的
- `—— 10 个字符，替换了 6 个（seed 0，intensity 1）` 是收尾摘要，告诉你一共读了几个字符、换了几个、用的什么参数。一个都没换时它会给出可能的原因（intensity 为 0，或表里没有这些字的映射），而不是干巴巴报个 0

  注意字符数**把换行也算在内** —— Windows 控制台给出的是 `\r\n`，所以上面两行输入是 `3 + 2 + 3 + 2 = 10` 个字符，而不是 6 个

**这些提示全部走 stderr**，所以 `marspeak-cli encode < a.txt > b.txt` 拿到的 `b.txt` 是干净的 —— 提示只在你直接看着终端时才出现。给了位置参数时也不会多打那行 `——— 结果 ———`，因为没有歧义。

### encode 的选项

| 选项 | 默认 | 含义 |
|---|---|---|
| `--seed <N>` | `0` | 随机种子。决定多候选的字挑中哪一个 |
| `--intensity <F>` | `1.0` | 替换强度，`0.0..=1.0`。约为「百分之多少的有映射字符被替换」 |
| `--table <PATH>` | 内置表 | 自定义映射表（TSV） |

### decode 的选项

| 选项 | 默认 | 含义 |
|---|---|---|
| `--table <PATH>` | 内置表 | 自定义映射表（TSV）。必须与 encode 时用的是同一张 |

**`decode` 不接受 `--seed` 和 `--intensity`** —— 还原是确定性的，跟这两个参数无关。传了会直接报未知参数，而不是静默忽略。

## 退出码

| 码 | 含义 |
|---|---|
| `0` | 成功 |
| `1` | 运行错误（强度越界、表文件读不到……） |
| `2` | 用法错误（参数写错、缺子命令） |

## 自定义映射表

每行 `原字<Tab>候选[,候选...]`，`#` 开头是注释：

```
# 注释以 # 开头，空行跳过
我	莪,沃
爱	愛
```

```bash
$ marspeak-cli encode --table my.tsv "我爱"
莪愛
$ marspeak-cli decode --table my.tsv "莪愛"
我爱
```

指定 `--table` 后内置表**完全不参与**，表里没定义的字原样通过。表格式与校验规则的完整说明见 [marspeak 的 README](../lib/README.md#自定义映射表)。

## Windows 上的中文输出

程序输出的是 UTF-8。终端代码页不是 65001 时会显示乱码 —— 先执行：

```cmd
chcp 65001
```

**PowerShell 用管道喂中文要额外一步**：PowerShell 5.1 的 `$OutputEncoding` 默认是 `us-ascii`，中文在进程序之前就被换成 `?` 了。先设置一次：

```powershell
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)
echo "我爱你" | marspeak-cli encode
```

或者直接改用位置参数 / 文件重定向，两者都不受影响。

## 许可

MIT
