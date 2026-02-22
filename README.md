# mini-sysctl

sysctl.conf 形式の設定ファイルをパースし、スキーマに基づいて検証するライブラリ。

## 設定ファイルの形式

```conf
# コメント（# または ; で開始）
endpoint = localhost:3000
debug = true
log.file = /var/log/console.log

# 先頭に - を付けるとエラー時に無視
-net.ipv4.conf.default.rp_filter = 1
```

- `key = value` 形式
- `#` または `;` で始まる行はコメント
- 空行は無視
- 先頭 `-` 付きのキーは `ignore_error` フラグが立つ
- 値に `=` を含められる（最初の `=` で分割）

## スキーマファイルの形式

```conf
endpoint = string
debug = bool
log.file = string
retry = integer
```

設定ファイルと同じ `key = value` 形式で、値の部分に型名を記述する。

使える型:
- `string` — 任意の文字列
- `bool` — `true` または `false`
- `integer` — 64bit 整数

コメントや空行も使用可能。

## 使い方

```rust
use mini_sysctl::{Config, Schema, validate};

// 設定ファイルをパース
let config = Config::parse("endpoint = localhost:3000\nretry = 3").unwrap();

// スキーマファイルをパース
let schema = Schema::parse("endpoint = string\nretry = integer").unwrap();

// 検証
match validate(&config, &schema) {
    Ok(()) => println!("OK"),
    Err(errors) => {
        for e in &errors {
            eprintln!("{}", e);
        }
    }
}
```

## 検証エラーの種類

| エラー | 意味 |
|--------|------|
| `TypeMismatch` | 値がスキーマで指定された型に合わない |
| `UnknownKey` | スキーマに定義されていないキーが設定にある |
| `MissingKey` | スキーマで定義されたキーが設定に存在しない |

## テスト

```sh
cargo test
```
