use std::collections::HashMap;
use std::fmt;

// === エラー型 ===

#[derive(Debug)]
pub enum ParseError {
    InvalidLine { line_number: usize, content: String },
    InvalidType { line_number: usize, type_name: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidLine { line_number, content } => {
                write!(f, "line {}: invalid syntax: {}", line_number, content)
            }
            ParseError::InvalidType { line_number, type_name } => {
                write!(f, "line {}: unknown type: {}", line_number, type_name)
            }
        }
    }
}

impl std::error::Error for ParseError {}

// === Token ===

#[derive(Debug)]
enum Token {
    Comment(String),
    BlankLine,
    KeyValue {
        key: String,
        value: String,
        ignore_error: bool,
    },
}

#[derive(Debug)]
struct Tokens {
    tokens: Vec<Token>,
}

impl Tokens {
    fn parse(content: &str) -> Result<Self, ParseError> {
        let nodes = content
            .lines()
            .enumerate()
            .map(|(i, line)| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    Ok(Token::BlankLine)
                } else if trimmed.starts_with('#') || trimmed.starts_with(';') {
                    Ok(Token::Comment(trimmed.to_string()))
                } else if let Some(rest) = trimmed.strip_prefix('-') {
                    let (key, value) = rest.split_once('=').ok_or(ParseError::InvalidLine {
                        line_number: i + 1,
                        content: line.to_string(),
                    })?;
                    Ok(Token::KeyValue {
                        key: key.trim().to_string(),
                        value: value.trim().to_string(),
                        ignore_error: true,
                    })
                } else {
                    let (key, value) =
                        trimmed
                            .split_once('=')
                            .ok_or(ParseError::InvalidLine {
                                line_number: i + 1,
                                content: line.to_string(),
                            })?;
                    Ok(Token::KeyValue {
                        key: key.trim().to_string(),
                        value: value.trim().to_string(),
                        ignore_error: false,
                    })
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Tokens { tokens: nodes })
    }
}

// === Config ===

#[derive(Debug)]
pub struct Config {
    entries: HashMap<String, String>,
}

impl Config {
    pub fn parse(content: &str) -> Result<Self, ParseError> {
        let entries = Tokens::parse(content)?
            .tokens
            .into_iter()
            .filter_map(|node| match node {
                Token::KeyValue { key, value, .. } => Some((key, value)),
                _ => None,
            })
            .collect();

        Ok(Config { entries })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }
}

// === 検証 ===

#[derive(Debug)]
pub enum ValueType {
    Str,
    Bool,
    Integer,
}

impl ValueType {
    fn is_valid(&self, value: &str) -> bool {
        match self {
            ValueType::Str => true,
            ValueType::Bool => value == "true" || value == "false",
            ValueType::Integer => value.parse::<i64>().is_ok(),
        }
    }
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueType::Str => write!(f, "string"),
            ValueType::Bool => write!(f, "bool"),
            ValueType::Integer => write!(f, "integer"),
        }
    }
}

#[derive(Debug)]
pub struct Schema {
    entries: HashMap<String, ValueType>,
}

impl Schema {
    pub fn parse(content: &str) -> Result<Self, ParseError> {
        let parsed = Tokens::parse(content)?;
        let mut entries = HashMap::new();
        for (i, token) in parsed.tokens.into_iter().enumerate() {
            if let Token::KeyValue { key, value, .. } = token {
                let vt = match value.as_str() {
                    "string" => ValueType::Str,
                    "bool" => ValueType::Bool,
                    "integer" => ValueType::Integer,
                    other => {
                        return Err(ParseError::InvalidType {
                            line_number: i + 1,
                            type_name: other.to_string(),
                        })
                    }
                };
                entries.insert(key, vt);
            }
        }
        Ok(Schema { entries })
    }
}

#[derive(Debug)]
pub enum ValidationError {
    TypeMismatch {
        key: String,
        expected: String,
        got: String,
    },
    UnknownKey {
        key: String,
    },
    MissingKey {
        key: String,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::TypeMismatch { key, expected, got } => {
                write!(f, "'{}': expected {}, got '{}'", key, expected, got)
            }
            ValidationError::UnknownKey { key } => {
                write!(f, "'{}': unknown key (not in schema)", key)
            }
            ValidationError::MissingKey { key } => {
                write!(f, "'{}': missing (required by schema)", key)
            }
        }
    }
}

pub fn validate(config: &Config, schema: &Schema) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    // schema の各keyについて config の値を型チェック
    for (key, vt) in &schema.entries {
        match config.get(key) {
            Some(value) => {
                if !vt.is_valid(value) {
                    errors.push(ValidationError::TypeMismatch {
                        key: key.clone(),
                        expected: vt.to_string(),
                        got: value.to_string(),
                    });
                }
            }
            None => {
                errors.push(ValidationError::MissingKey { key: key.clone() });
            }
        }
    }

    // config に schema にないkeyがあれば UnknownKey
    for key in config.entries.keys() {
        if !schema.entries.contains_key(key) {
            errors.push(ValidationError::UnknownKey { key: key.clone() });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// === テスト ===

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // --- ジェネレータ ---

    // "foo", "bar.baz", "a1.b2.c3.d4" のようなドット区切りキー
    fn arb_key() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9]{0,10}(\\.[a-z][a-z0-9]{0,10}){0,3}"
    }

    // 英数字と一部記号からなる任意の値（= は値に含まれうる。パーサーは最初の = で分割する）
    fn arb_value() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9./_, =-]{0,50}"
    }

    // "key = value" 形式の1行
    fn arb_key_value_line() -> impl Strategy<Value = String> {
        (arb_key(), arb_value()).prop_map(|(k, v)| format!("{} = {}", k, v))
    }

    // "#..." または ";..." 形式のコメント行
    fn arb_comment_line() -> impl Strategy<Value = String> {
        prop_oneof![
            "[#][a-zA-Z0-9 ]{0,50}",
            "[;][a-zA-Z0-9 ]{0,50}",
        ]
    }

    // KV行・コメント行・空行をランダムに混ぜた設定ファイル全体
    fn arb_config_content() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop_oneof![
                arb_key_value_line(),
                arb_comment_line(),
                Just("".to_string()),
            ],
            0..30,
        )
        .prop_map(|lines| lines.join("\n"))
    }

    // --- パース: 有効な入力は常にパースできる ---

    proptest! {
        #[test]
        fn valid_config_always_parses(content in arb_config_content()) {
            prop_assert!(Config::parse(&content).is_ok());
        }
    }

    // --- 検証: validate の結果は config と schema の差異から決定的に導出できる ---

    proptest! {
        #[test]
        // スキーマに完全に適合する config は常に検証を通る
        fn matching_config_always_passes(
            int_val in any::<i64>().prop_map(|n| n.to_string()),
            bool_val in prop_oneof![Just("true"), Just("false")],
            str_val in "[a-zA-Z0-9]{1,50}",
        ) {
            let config = Config::parse(&format!("\
retry = {}
debug = {}
endpoint = {}", int_val, bool_val, str_val)).unwrap();
            let schema = Schema::parse("\
retry = integer
debug = bool
endpoint = string").unwrap();
            prop_assert!(validate(&config, &schema).is_ok());
        }

    }

    // --- 検証: 値がスキーマの型に合わなければ TypeMismatch ---

    #[test]
    fn string_value_for_integer_schema_is_type_mismatch() {
        // "abc" は integer として不正
        let config = Config::parse("retry = abc").unwrap();
        let schema = Schema::parse("retry = integer").unwrap();
        let errors = validate(&config, &schema).unwrap_err();
        assert!(matches!(&errors[0], ValidationError::TypeMismatch { key, .. } if key == "retry"));
    }

    // --- 検証: スキーマにないキーがあれば UnknownKey ---

    #[test]
    fn key_not_in_schema_is_unknown() {
        // extra はスキーマに定義されていない
        let config = Config::parse("\
retry = 1
extra = x").unwrap();
        let schema = Schema::parse("retry = integer").unwrap();
        let errors = validate(&config, &schema).unwrap_err();
        assert!(matches!(&errors[0], ValidationError::UnknownKey { key } if key == "extra"));
    }

    // --- スキーマパース ---

    #[test]
    fn schema_ignores_comments_and_blank_lines() {
        let schema = Schema::parse("\
# ネットワーク設定
endpoint = string

# デバッグ
debug = bool").unwrap();
        assert_eq!(schema.entries.len(), 2);
    }

    #[test]
    fn schema_rejects_unknown_type_name() {
        // "number" はサポート外の型名
        let err = Schema::parse("retry = number").unwrap_err();
        assert!(matches!(err, ParseError::InvalidType { line_number: 1, .. }));
    }

    // --- 入力例による結合テスト ---

    #[test]
    fn missing_key_detected_when_config_lacks_schema_entry() {
        // log.name がスキーマにあるが設定にない → MissingKey
        let config = Config::parse("\
endpoint = localhost:3000
debug = true
log.file = /var/log/console.log").unwrap();
        let schema = Schema::parse("\
endpoint = string
debug = bool
log.file = string
log.name = string").unwrap();
        let errors = validate(&config, &schema).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(&errors[0], ValidationError::MissingKey { key } if key == "log.name"));
    }

    #[test]
    fn commented_out_key_is_treated_as_missing() {
        // debug がコメントアウトされている → パーサーは無視 → MissingKey
        let config = Config::parse("\
endpoint = localhost:3000
# debug = true
log.file = /var/log/console.log
log.name = default.log").unwrap();
        let schema = Schema::parse("\
endpoint = string
debug = bool
log.file = string
log.name = string").unwrap();
        let errors = validate(&config, &schema).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(&errors[0], ValidationError::MissingKey { key } if key == "debug"));
    }
}
