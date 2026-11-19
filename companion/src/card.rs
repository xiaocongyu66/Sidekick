//! SillyTavern 角色卡导入与解析（V2/V3，PNG 内嵌或直读 JSON）。
//!
//! - PNG：读 tEXt/iTXt chunk 里 keyword 为 `ccv3`（V3，优先）或
//!   `chara`（V2）的 base64 负载。
//! - JSON：`spec` 字段区分 V2（`chara_card_v2`）与 V3（`chara_card_v3`），
//!   或扁平 V1 卡。两版 data 结构向前兼容地取并集。
//! - 「决策卡」对应 character_book（world info）：条目带 keys/constant/
//!   selective，运行时按近期消息命中情况选择注入（世界设定）。

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

/// PNG 签名。
const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
/// 角色卡在 PNG 元数据 chunk 里的 keyword（V2）。
const CARDA_KEYWORD: &str = "chara";
/// V3 卡的 keyword（优先于 `chara`）。
const CARDA_V3_KEYWORD: &str = "ccv3";

// --- 数据模型 ---------------------------------------------------------------

/// 一张解析后的角色卡（UI 与运行时使用）。
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // 卡面元数据：解析保留，UI 展示与卡片切换时再用。
pub(crate) struct CharacterCard {
    pub(crate) spec: CardSpec,
    pub(crate) name: String,
    /// V3：`{{char}}` 展开为它（空则回退 name）。
    pub(crate) nickname: String,
    pub(crate) description: String,
    pub(crate) personality: String,
    pub(crate) scenario: String,
    pub(crate) first_mes: String,
    pub(crate) alternate_greetings: Vec<String>,
    pub(crate) mes_example: String,
    pub(crate) system_prompt: String,
    pub(crate) post_history_instructions: String,
    pub(crate) creator_notes: String,
    pub(crate) tags: Vec<String>,
    pub(crate) creator: String,
    pub(crate) character_version: String,
    /// V3 新增设定字段。
    pub(crate) identity: String,
    pub(crate) background: String,
    pub(crate) appearance: String,
    pub(crate) group_only_greetings: Vec<String>,
    /// 世界书/决策卡。
    pub(crate) book: Option<CharacterBook>,
}

/// 规格版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CardSpec {
    #[default]
    V1,
    V2,
    V3,
}

/// 世界书（决策卡）：条目集合 + 运行时参数。
#[derive(Debug, Clone, Default)]
pub(crate) struct CharacterBook {
    /// 条目的「名字」（与角色卡名区分，仅标识用）。
    #[allow(dead_code)]
    pub(crate) name: String,
    /// 触发扫描的最近消息深度（条数）。
    pub(crate) scan_depth: Option<u32>,
    pub(crate) entries: Vec<BookEntry>,
}

/// 一条世界书条目（决策卡的一张「卡」）。
#[derive(Debug, Clone, Default)]
pub(crate) struct BookEntry {
    pub(crate) keys: Vec<String>,
    /// 副键（secondary keys）：主键命中后作为 AND 二次过滤（ST 语义）。
    pub(crate) secondary_keys: Vec<String>,
    pub(crate) content: String,
    /// 始终注入（不依赖关键词命中）。
    pub(crate) constant: bool,
    /// 仅当主键命中时才考虑注入。
    pub(crate) selective: bool,
    /// 排序权重：小的先注入。
    pub(crate) insertion_order: i32,
    pub(crate) case_sensitive: bool,
    pub(crate) enabled: bool,
    /// 注入位置：角色定义前/后。
    pub(crate) position: EntryPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum EntryPosition {
    /// before_char：角色描述之前。
    #[default]
    BeforeChar,
    /// after_char：角色描述之后。
    AfterChar,
    /// before_an：作者注之前。
    BeforeAuthorNote,
    /// after_an：作者注之后。
    AfterAuthorNote,
    /// @depth：按深度插在对话历史中，本实现并入前置档。
    AtDepth,
}

// --- 反序列化 ---------------------------------------------------------------

/// 反序列化中间层：缺字段给默认，未知字段忽略（未知量保留在 `raw` 里）。
mod de {
    use super::*;

    #[derive(Deserialize)]
    pub(crate) struct RawCard {
        #[serde(default)]
        spec: String,
        #[serde(default)]
        spec_version: String,
        /// V2/V3：核心数据在 `data` 下；扁平卡（V1）在根。
        #[serde(default)]
        data: Option<Value>,
        #[serde(default)]
        name: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        personality: String,
        #[serde(default)]
        scenario: String,
        #[serde(default)]
        first_mes: String,
        #[serde(default)]
        mes_example: String,
    }

    fn text(value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            _ => value.to_string(),
        }
    }

    /// 字符串或数组都收成 Vec<String>。
    fn strings(value: &Value) -> Vec<String> {
        match value {
            Value::String(s) => vec![s.clone()],
            Value::Array(items) => items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect(),
            _ => Vec::new(),
        }
    }

    impl RawCard {
        pub(crate) fn into_card(self) -> CharacterCard {
            let data = self.data.unwrap_or_else(|| {
                // 扁平卡：把根字段拼一个伪 data。
                serde_json::json!({
                    "name": self.name,
                    "description": self.description,
                    "personality": self.personality,
                    "scenario": self.scenario,
                    "first_mes": self.first_mes,
                    "mes_example": self.mes_example,
                })
            });
            let spec = match self.spec.as_str() {
                "chara_card_v2" => CardSpec::V2,
                "chara_card_v3" => CardSpec::V3,
                _ => match self.spec_version.as_str() {
                    s if s.starts_with("3") => CardSpec::V3,
                    s if s.starts_with("2") => CardSpec::V2,
                    _ => CardSpec::V1,
                },
            };
            CharacterCard {
                spec,
                nickname: text(&data["nickname"]),
                name: text(&data["name"]),
                description: text(&data["description"]),
                personality: text(&data["personality"]),
                scenario: text(&data["scenario"]),
                first_mes: text(&data["first_mes"]),
                alternate_greetings: strings(&data["alternate_greetings"]),
                mes_example: text(&data["mes_example"]),
                system_prompt: text(&data["system_prompt"]),
                post_history_instructions: text(&data["post_history_instructions"]),
                creator_notes: text(&data["creator_notes"]),
                tags: strings(&data["tags"]),
                creator: text(&data["creator"]),
                character_version: text(&data["character_version"]),
                identity: text(&data["identity"]),
                background: text(&data["background"]),
                appearance: text(&data["appearance"]),
                group_only_greetings: strings(&data["group_only_greetings"]),
                book: data["character_book"].as_object().and_then(parse_book),
            }
        }
    }
}

fn parse_book(obj: &serde_json::Map<String, Value>) -> Option<CharacterBook> {
    let entries = match obj.get("entries") {
        Some(Value::Array(items)) => items.iter().filter_map(parse_entry).collect(),
        // 部分 JSON 导出器用对象 keyed by id。
        Some(Value::Object(map)) => map.values().filter_map(parse_entry).collect(),
        _ => Vec::new(),
    };
    Some(CharacterBook {
        name: obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        scan_depth: obj.get("scan_depth").and_then(Value::as_u64).map(|n| n as u32),
        entries,
    })
}

fn parse_entry(value: &Value) -> Option<BookEntry> {
    let obj = value.as_object()?;
    let enabled = obj
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| !obj.get("disable").and_then(Value::as_bool).unwrap_or(false));
    let extensions = obj.get("extensions").and_then(Value::as_object);
    let position = match extensions.and_then(|e| e.get("position")) {
        // ST 导出的卡把数值位置（0..7）放在 extensions.position。
        Some(Value::Number(n)) => match n.as_i64().unwrap_or(0) {
            1 | 3 | 5 | 6 => EntryPosition::AfterChar,
            2 => EntryPosition::BeforeAuthorNote,
            4 => EntryPosition::AtDepth,
            _ => EntryPosition::BeforeChar,
        },
        _ => match obj.get("position") {
            Some(Value::Number(n)) => match n.as_i64().unwrap_or(0) {
                1 => EntryPosition::AfterChar,
                2 => EntryPosition::BeforeAuthorNote,
                3 | 5 | 6 => EntryPosition::AfterAuthorNote,
                4 => EntryPosition::AtDepth,
                _ => EntryPosition::BeforeChar,
            },
            Some(Value::String(s)) => match s.as_str() {
                "after_char" => EntryPosition::AfterChar,
                "before_char" => EntryPosition::BeforeChar,
                "before_an" => EntryPosition::BeforeAuthorNote,
                "after_an" | "at_depth" => EntryPosition::AfterAuthorNote,
                _ => EntryPosition::BeforeChar,
            },
            _ => EntryPosition::BeforeChar,
        },
    };
    Some(BookEntry {
        keys: strings(&obj["keys"]),
        secondary_keys: strings(&obj["keysecondary"]),
        content: strip_decorators(text(&obj["content"])),
        constant: obj.get("constant").and_then(Value::as_bool).unwrap_or(false),
        selective: obj.get("selective").and_then(Value::as_bool).unwrap_or(false),
        insertion_order: obj
            .get("insertion_order")
            .and_then(Value::as_i64)
            .map(|n| n as i32)
            .unwrap_or(100),
        case_sensitive: obj.get("case_sensitive").and_then(Value::as_bool).unwrap_or(false),
        enabled,
        position,
    })
}

fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        _ => value.to_string(),
    }
}

fn strings(value: &Value) -> Vec<String> {
    match value {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

/// 剥离 V3 lorebook 条目 content 开头的 `@@` 装饰器行（深度/门控等指令，
/// 本实现按 before/after 位置注入，装饰器行直接去掉）。
fn strip_decorators(mut content: String) -> String {
    let kept: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim_start().starts_with("@@"))
        .collect();
    let trimmed = kept.join("\n");
    if trimmed.is_empty() {
        content.clear();
    } else {
        content = trimmed;
    }
    content
}

// --- PNG 解包 ---------------------------------------------------------------

/// 从任意字节中解析角色卡：PNG 走 chunk 提取，其它按 JSON 尝试。
pub(crate) fn parse_bytes(bytes: &[u8]) -> Result<CharacterCard> {
    if bytes.starts_with(&PNG_SIG) {
        let json = extract_png_card(bytes)?;
        parse_json(&json)
    } else {
        parse_json(bytes)
    }
}

/// 提取 PNG 的角色卡元数据：V3 用 `ccv3`，V2 用 `chara`，ccv3 优先。
/// 两者都可能存于 tEXt（ST 服务端实现）或 iTXt，base64 解码。
fn extract_png_card(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut at = PNG_SIG.len();
    while at + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
            as usize;
        let type_at = at + 4;
        if type_at + 4 > bytes.len() {
            break;
        }
        let chunk = &bytes[type_at..type_at + 4];
        let data_at = type_at + 4;
        let data_end = data_at + len;
        if data_end + 4 > bytes.len() {
            bail!("PNG chunk extends past end of file");
        }
        let data = &bytes[data_at..data_end];
        if chunk != b"tEXt" && chunk != b"iTXt" {
            if chunk == b"IEND" {
                break;
            }
            at = data_end + 4;
            continue;
        }
        // 两个 keyword × 两种 chunk 类型，ccv3 优先。
        for keyword in [CARDA_V3_KEYWORD, CARDA_KEYWORD] {
            let payload = if chunk == b"tEXt" {
                text_payload(data, keyword)
            } else {
                itxt_payload(data, keyword)
            };
            if let Some(payload) = payload {
                return decode_base64(&payload);
            }
        }
        at = data_end + 4;
    }
    bail!("no `chara`/`ccv3` metadata in PNG")
}

/// tEXt：`keyword\0text`。
fn text_payload(data: &[u8], keyword: &str) -> Option<Vec<u8>> {
    let split = data.iter().position(|b| *b == 0)?;
    let key = std::str::from_utf8(&data[..split]).ok()?;
    if key != keyword {
        return None;
    }
    Some(data[split + 1..].to_vec())
}

/// iTXt：`keyword\0 compression_flag compression_method lang\0 translated\0 text`。
/// ST 导出的角色卡 iTXt 不压缩（flag=0）。
fn itxt_payload(data: &[u8], keyword: &str) -> Option<Vec<u8>> {
    let split = data.iter().position(|b| *b == 0)?;
    let key = std::str::from_utf8(&data[..split]).ok()?;
    if key != keyword {
        return None;
    }
    let rest = &data[split + 1..];
    if rest.len() < 2 || rest[0] != 0 {
        return None; // 压缩的 iTXt 不支持（角色卡不走这条路）
    }
    // 跳过 compression flag/method（2 字节）。
    let rest = &rest[2..];
    let end_lang = rest.iter().position(|b| *b == 0)?;
    let rest = &rest[end_lang + 1..];
    let end_translated = rest.iter().position(|b| *b == 0)?;
    Some(rest[end_translated + 1..].to_vec())
}

fn decode_base64(payload: &[u8]) -> Result<Vec<u8>> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let text = std::str::from_utf8(payload).context("`chara` payload is not UTF-8")?;
    STANDARD
        .decode(text.trim_end_matches(['\n', '\r', ' ']))
        .map_err(|error| anyhow::anyhow!("`chara` base64 decode failed: {error}"))
}

pub(crate) fn parse_json(bytes: &[u8]) -> Result<CharacterCard> {
    let raw: Value = serde_json::from_slice(bytes).context("invalid character card JSON")?;
    let card: de::RawCard = serde_json::from_value(raw).context("character card shape mismatch")?;
    Ok(card.into_card())
}

// --- 提示组装与决策卡匹配 ---------------------------------------------------

/// 默认陪伴原则（卡片自带 system_prompt 时让位给卡片）。
const FALLBACK_SYSTEM: &str = "你是用户指定的角色，同时是一位温暖的对话伙伴。\
用日常口语，句子短、有温度；不主动执行任务，把对话留在人与人的温度上。";

/// 组装角色卡的系统提示：base + 角色定义 + 命中的决策卡条目。
///
/// `recent` 是近期消息文本（用于决策卡关键词命中扫描）。
pub(crate) fn render_system(card: &CharacterCard, recent: &[String]) -> String {
    let mut out = String::new();
    let base = if card.system_prompt.trim().is_empty() {
        FALLBACK_SYSTEM
    } else {
        card.system_prompt.trim()
    };
    out.push_str(base);
    out.push_str("\n\n— 角色卡 —\n");
    if !card.name.is_empty() {
        out.push_str("名字：");
        out.push_str(&macros(card, &card.name));
        out.push('\n');
    }
    for (label, value) in [
        ("身份", &card.identity),
        ("背景", &card.background),
        ("外貌", &card.appearance),
        ("描述", &card.description),
        ("性格", &card.personality),
        ("场景", &card.scenario),
    ] {
        if !value.trim().is_empty() {
            out.push_str(label);
            out.push('：');
            out.push_str(&macros(card, value.trim()));
            out.push('\n');
        }
    }
    if !card.mes_example.trim().is_empty() {
        out.push_str("\n对话示例：\n");
        out.push_str(&macros(card, card.mes_example.trim()));
    }
    // 决策卡注入。
    if let Some(book) = &card.book {
        let (before, after) = match_entries(book, recent);
        if !before.is_empty() {
            out.push_str("\n— 世界设定（前） —\n");
            for entry in before {
                out.push_str(&macros(card, entry.content.trim()));
                out.push_str("\n\n");
            }
        }
        if !after.is_empty() {
            out.push_str("\n— 世界设定（后） —\n");
            for entry in after {
                out.push_str(&macros(card, entry.content.trim()));
                out.push_str("\n\n");
            }
        }
    }
    if !card.post_history_instructions.trim().is_empty() {
        out.push_str("\n— 对话结束前要求 —\n");
        out.push_str(&macros(card, card.post_history_instructions.trim()));
    }
    out
}

/// 本轮命中的世界书条目：constant 全取 + selective 主键命中者，
/// 按 insertion_order 升序，再按 before/after 分组。
fn match_entries<'a>(
    book: &'a CharacterBook,
    recent: &[String],
) -> (Vec<&'a BookEntry>, Vec<&'a BookEntry>) {
    let depth = book.scan_depth.unwrap_or(4) as usize;
    let window: Vec<String> = if recent.len() > depth {
        recent[recent.len() - depth..].to_vec()
    } else {
        recent.to_vec()
    };
    let contains = |key: &str, entry: &BookEntry| -> bool {
        if entry.case_sensitive {
            window.iter().any(|text| text.contains(key))
        } else {
            let needle = key.to_lowercase();
            window.iter().any(|text| text.to_lowercase().contains(&needle))
        }
    };
    let mut hits: Vec<&BookEntry> = book
        .entries
        .iter()
        .filter(|entry| {
            if !entry.enabled {
                return false;
            }
            if entry.constant {
                return true;
            }
            if !entry.selective || entry.keys.is_empty() {
                return false;
            }
            let main_hit = entry.keys.iter().any(|key| contains(key, entry));
            if !main_hit {
                return false;
            }
            // 副键非空：主键命中后还须命中任一副键（ST 语义）。
            entry.secondary_keys.is_empty()
                || entry.secondary_keys.iter().any(|key| contains(key, entry))
        })
        .collect();
    hits.sort_by_key(|entry| entry.insertion_order);
    let mut before = Vec::new();
    let mut after = Vec::new();
    for entry in hits {
        match entry.position {
            EntryPosition::BeforeChar | EntryPosition::BeforeAuthorNote | EntryPosition::AtDepth => {
                before.push(entry)
            }
            EntryPosition::AfterChar | EntryPosition::AfterAuthorNote => after.push(entry),
        }
    }
    (before, after)
}

/// CBS 宏替换：`{{char}}` → nickname/name，`{{user}}` → 我。
fn macros(card: &CharacterCard, text: &str) -> String {
    let char_name = if card.nickname.is_empty() {
        &card.name
    } else {
        &card.nickname
    };
    text.replace("{{char}}", char_name).replace("{{user}}", "我")
}

/// 下一张开场白：先 first_mes，再 alternate_greetings。index 超出回 0。
pub(crate) fn greeting(card: &CharacterCard, index: usize) -> &str {
    if index == 0 {
        &card.first_mes
    } else {
        card.alternate_greetings
            .get(index - 1)
            .map(String::as_str)
            .unwrap_or(&card.first_mes)
    }
}

/// 应用宏替换后的开场白（UI 直接显示）。
pub(crate) fn greeting_macros(card: &CharacterCard, index: usize) -> String {
    macros(card, greeting(card, index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v2_json() -> &'static str {
        r#"{
            "spec": "chara_card_v2",
            "spec_version": "2.0",
            "data": {
                "name": "艾拉",
                "nickname": "小艾",
                "description": "流浪剑客",
                "personality": "冷淡，话少",
                "scenario": "酒馆相遇",
                "first_mes": "……{{char}}盯着我看做什么？",
                "alternate_greetings": ["又见面了。"],
                "mes_example": "「嗯。」",
                "tags": ["奇幻"],
                "character_book": {
                    "name": "世界观",
                    "entries": [
                        {"keys": ["剑", "佩剑"], "content": "{{char}}腰间挂着一把断剑", "constant": false, "selective": true, "insertion_order": 10, "position": "after_char"},
                        {"keys": ["酒馆"], "content": "酒馆叫「残月」", "constant": true, "position": 0},
                        {"keys": ["森林", "林子"], "content": "@@depth 1\n森林里有狼。", "selective": true}
                    ]
                }
            }
        }"#
    }

    #[test]
    fn parses_v2_json() {
        let card = parse_json(v2_json().as_bytes()).unwrap();
        assert_eq!(card.name, "艾拉");
        assert_eq!(card.spec, CardSpec::V2);
        assert_eq!(card.nickname, "小艾");
        assert_eq!(card.first_mes, "……{{char}}盯着我看做什么？");
        assert_eq!(card.alternate_greetings, vec!["又见面了。"]);
        let book = card.book.unwrap();
        assert_eq!(book.entries.len(), 3);
        let sword = &book.entries[0];
        assert_eq!(sword.position, EntryPosition::AfterChar);
        assert!(sword.selective);
        let tavern = &book.entries[1];
        assert!(tavern.constant);
        assert_eq!(tavern.position, EntryPosition::BeforeChar);
        // @@ 装饰器行被剥离。
        assert_eq!(book.entries[2].content, "森林里有狼。");
    }

    #[test]
    fn render_replaces_macros_and_matches_entries() {
        let card = parse_json(v2_json().as_bytes()).unwrap();
        let recent = vec!["你腰间的佩剑断了一截".to_string()];
        let system = render_system(&card, &recent);
        assert!(system.contains("艾拉"), "system: {system}");
        assert!(system.contains("小艾") || system.contains("艾拉"));
        // 命中「佩剑」→ after 档条目，宏替换后。
        assert!(system.contains("小艾腰间挂着一把断剑"), "system: {system}");
        // constant 条目（酒馆）一定进。
        assert!(system.contains("酒馆叫「残月」"), "system: {system}");
    }

    #[test]
    fn render_without_recent_skips_selective() {
        let card = parse_json(v2_json().as_bytes()).unwrap();
        let system = render_system(&card, &[]);
        assert!(!system.contains("断剑"));
        // constant 仍在。
        assert!(system.contains("残月"));
    }

    #[test]
    fn greeting_macros_applied() {
        let card = parse_json(v2_json().as_bytes()).unwrap();
        assert_eq!(greeting_macros(&card, 0), "……小艾盯着我看做什么？");
        assert_eq!(greeting_macros(&card, 1), "又见面了。");
    }

    /// 最小 PNG：签名 + IEND，无 chara → 应失败而不是 panic。
    #[test]
    fn png_without_card_fails() {
        let png = [PNG_SIG.as_slice(), b"\x00\x00\x00\x00IEND\xAE\x42\x60\x82"].concat();
        let error = extract_png_card(&png).unwrap_err().to_string();
        assert!(error.contains("chara"), "{error}");
    }

    #[test]
    fn text_and_itxt_payloads_match() {
        let data = b"chara\0eyJhIjoxfQ==";
        assert_eq!(text_payload(data, "chara").unwrap(), b"eyJhIjoxfQ==");
        let itxt = b"chara\0\x00\x00en\0translated\0eyJiIjoyfQ==";
        assert_eq!(itxt_payload(itxt, "chara").unwrap(), b"eyJiIjoyfQ==");
        // ccv3 keyword。
        assert_eq!(text_payload(b"ccv3\0e30=", "ccv3").unwrap(), b"e30=");
    }

    #[test]
    fn png_round_trip() {
        let json = v2_json().as_bytes();
        use base64::{Engine, engine::general_purpose::STANDARD};
        let b64 = STANDARD.encode(json);
        let chunk: Vec<u8> = b"chara\0".to_vec()
            [..]
            .iter()
            .chain(b64.as_bytes().iter())
            .copied()
            .collect();
        let len = chunk.len() as u32;
        let mut png = PNG_SIG.to_vec();
        png.extend_from_slice(&len.to_be_bytes());
        png.extend_from_slice(b"tEXt");
        png.extend_from_slice(&chunk);
        png.extend_from_slice(&[0; 4]); // crc（本解析器不校验）
        png.extend_from_slice(&[0, 0, 0, 0]);
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0xAE, 0x42, 0x60, 0x82]);
        let card = parse_bytes(&png).unwrap();
        assert_eq!(card.name, "艾拉");
    }
}
