//! Inline Emoji & Symbol picker (Issue #14): compact static Unicode dataset, `:` Root trigger,
//! and the `emoji` View.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, ItemId, ViewId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::view_source::{Preview, ViewSource};
use std::sync::Arc;

pub const VIEW_ID: &str = "emoji";
pub const OPEN_CMD_ID: &str = "emoji.open";
const ID_PREFIX: &str = "emoji:";

struct EmojiEntry {
    glyph: &'static str,
    slug: &'static str,
    name: &'static str,
    keywords: &'static str,
    category: &'static str,
}

const fn e(
    glyph: &'static str,
    slug: &'static str,
    name: &'static str,
    keywords: &'static str,
    category: &'static str,
) -> EmojiEntry {
    EmojiEntry {
        glyph,
        slug,
        name,
        keywords,
        category,
    }
}

/// Compact embedded Unicode emoji + developer/UI symbol dataset (R-006: lives in `.rodata`, zero heap).
const ENTRIES: &[EmojiEntry] = &[
    // High-frequency reactions & common emojis first (shown for empty / `:` query).
    e(
        "🔥",
        "fire",
        "Fire",
        "hot lit flame burn trending",
        "Smileys & Emotion",
    ),
    e(
        "🚀",
        "rocket",
        "Rocket",
        "launch ship deploy fast space",
        "Travel & Places",
    ),
    e(
        "❤️",
        "heart",
        "Red Heart",
        "love like favorite",
        "Smileys & Emotion",
    ),
    e(
        "👍",
        "thumbs_up",
        "Thumbs Up",
        "ok yes approve like +1",
        "People & Body",
    ),
    e(
        "✅",
        "check_mark_button",
        "Check Mark Button",
        "check done ok yes pass success",
        "Symbols",
    ),
    e(
        "✨",
        "sparkles",
        "Sparkles",
        "magic clean shine stars new",
        "Activities",
    ),
    e(
        "⚠️",
        "warning",
        "Warning",
        "alert caution danger sign",
        "Symbols",
    ),
    e(
        "😊",
        "smile",
        "Smiling Face",
        "happy warm blush friendly",
        "Smileys & Emotion",
    ),
    e(
        "😂",
        "joy",
        "Tears of Joy",
        "laugh lol funny cry",
        "Smileys & Emotion",
    ),
    e(
        "🎉",
        "party",
        "Party Popper",
        "tada celebrate congrats confetti",
        "Activities",
    ),
    e(
        "💡",
        "bulb",
        "Light Bulb",
        "idea tip bright insight",
        "Objects",
    ),
    e(
        "🐛",
        "bug",
        "Bug",
        "insect issue defect debug",
        "Animals & Nature",
    ),
    e(
        "⚡",
        "zap",
        "High Voltage",
        "lightning fast bolt power electric",
        "Nature & Weather",
    ),
    e(
        "👀",
        "eyes",
        "Eyes",
        "look watch see review",
        "People & Body",
    ),
    e(
        "🙏",
        "pray",
        "Folded Hands",
        "thanks please hope gratitude",
        "People & Body",
    ),
    e(
        "💯",
        "100",
        "Hundred Points",
        "perfect score full",
        "Smileys & Emotion",
    ),
    // Smileys & People
    e(
        "😀",
        "grin",
        "Grinning Face",
        "smile happy cheerful",
        "Smileys & Emotion",
    ),
    e(
        "😁",
        "beaming",
        "Beaming Face",
        "grin happy teeth",
        "Smileys & Emotion",
    ),
    e(
        "😅",
        "sweat_smile",
        "Grinning Sweat",
        "nervous relief pheww",
        "Smileys & Emotion",
    ),
    e(
        "🤣",
        "rofl",
        "Rolling Laughing",
        "lmao funny laugh",
        "Smileys & Emotion",
    ),
    e(
        "😉",
        "wink",
        "Winking Face",
        "flirt joke playful",
        "Smileys & Emotion",
    ),
    e(
        "😍",
        "heart_eyes",
        "Heart Eyes",
        "love adore crush",
        "Smileys & Emotion",
    ),
    e(
        "🥰",
        "smiling_hearts",
        "Smiling with Hearts",
        "love warm affection",
        "Smileys & Emotion",
    ),
    e(
        "😎",
        "sunglasses",
        "Smiling with Sunglasses",
        "cool chill awesome",
        "Smileys & Emotion",
    ),
    e(
        "🤔",
        "thinking",
        "Thinking Face",
        "hmm ponder question wonder",
        "Smileys & Emotion",
    ),
    e(
        "🫡",
        "salute",
        "Saluting Face",
        "respect aye sir",
        "Smileys & Emotion",
    ),
    e(
        "🤯",
        "mind_blown",
        "Exploding Head",
        "wow amazed shocked",
        "Smileys & Emotion",
    ),
    e(
        "🥳",
        "partying",
        "Partying Face",
        "celebrate birthday hat",
        "Smileys & Emotion",
    ),
    e(
        "😭",
        "sob",
        "Loudly Crying",
        "sad tears grief",
        "Smileys & Emotion",
    ),
    e(
        "😤",
        "triumph",
        "Face with Steam",
        "huff determined annoyed",
        "Smileys & Emotion",
    ),
    e(
        "😡",
        "angry",
        "Angry Face",
        "mad rage furious",
        "Smileys & Emotion",
    ),
    e(
        "💀",
        "skull",
        "Skull",
        "dead rip skeleton",
        "Smileys & Emotion",
    ),
    e(
        "👎",
        "thumbs_down",
        "Thumbs Down",
        "no dislike reject -1",
        "People & Body",
    ),
    e(
        "👏",
        "clap",
        "Clapping Hands",
        "applause bravo congrats",
        "People & Body",
    ),
    e(
        "👋",
        "wave",
        "Waving Hand",
        "hello hi bye greeting",
        "People & Body",
    ),
    e(
        "🙌",
        "raised_hands",
        "Raising Hands",
        "hooray praise celebrate",
        "People & Body",
    ),
    e(
        "💪",
        "muscle",
        "Flexed Biceps",
        "strong power gym",
        "People & Body",
    ),
    e(
        "👌",
        "ok_hand",
        "OK Hand",
        "perfect good fine",
        "People & Body",
    ),
    e(
        "🤝",
        "handshake",
        "Handshake",
        "deal agree partner",
        "People & Body",
    ),
    e(
        "👉",
        "point_right",
        "Pointing Right",
        "direction finger here",
        "People & Body",
    ),
    e(
        "👈",
        "point_left",
        "Pointing Left",
        "direction finger back",
        "People & Body",
    ),
    e(
        "👆",
        "point_up",
        "Pointing Up",
        "above finger this",
        "People & Body",
    ),
    e(
        "👇",
        "point_down",
        "Pointing Down",
        "below finger look",
        "People & Body",
    ),
    // Hearts & Reactions
    e(
        "🧡",
        "orange_heart",
        "Orange Heart",
        "love warm",
        "Smileys & Emotion",
    ),
    e(
        "💛",
        "yellow_heart",
        "Yellow Heart",
        "love gold friendship",
        "Smileys & Emotion",
    ),
    e(
        "💚",
        "green_heart",
        "Green Heart",
        "love nature envy",
        "Smileys & Emotion",
    ),
    e(
        "💙",
        "blue_heart",
        "Blue Heart",
        "love trust calm",
        "Smileys & Emotion",
    ),
    e(
        "💜",
        "purple_heart",
        "Purple Heart",
        "love royal",
        "Smileys & Emotion",
    ),
    e(
        "🖤",
        "black_heart",
        "Black Heart",
        "love dark",
        "Smileys & Emotion",
    ),
    e(
        "💔",
        "broken_heart",
        "Broken Heart",
        "sad breakup hurt",
        "Smileys & Emotion",
    ),
    e("⭐", "star", "Star", "favorite bookmark rating", "Symbols"),
    e(
        "🌟",
        "glowing_star",
        "Glowing Star",
        "shine bright",
        "Symbols",
    ),
    e(
        "💥",
        "boom",
        "Collision",
        "bang crash explosion",
        "Smileys & Emotion",
    ),
    e(
        "💬",
        "speech_balloon",
        "Speech Balloon",
        "chat comment message",
        "Smileys & Emotion",
    ),
    e(
        "💭",
        "thought_balloon",
        "Thought Balloon",
        "idea think dream",
        "Smileys & Emotion",
    ),
    // Status & Symbols
    e(
        "❌",
        "cross_mark",
        "Cross Mark",
        "no wrong fail cancel error",
        "Symbols",
    ),
    e(
        "🚫",
        "prohibited",
        "Prohibited",
        "ban forbid block stop",
        "Symbols",
    ),
    e(
        "❓",
        "question",
        "Question Mark",
        "help ask doubt",
        "Symbols",
    ),
    e(
        "❗",
        "exclamation",
        "Exclamation Mark",
        "alert important bang",
        "Symbols",
    ),
    e("ℹ️", "info", "Information", "about help details", "Symbols"),
    e("🔔", "bell", "Bell", "notify alert reminder", "Objects"),
    e("📌", "pushpin", "Pushpin", "pin tack save", "Objects"),
    e("🔗", "link", "Link", "url chain connect", "Objects"),
    e("🔒", "lock", "Locked", "secure private closed", "Objects"),
    e("🔓", "unlock", "Unlocked", "open access", "Objects"),
    e("🔑", "key", "Key", "secret password auth", "Objects"),
    e(
        "🔍",
        "mag",
        "Magnifying Glass",
        "search find inspect zoom",
        "Objects",
    ),
    e(
        "⏳",
        "hourglass",
        "Hourglass",
        "wait loading time pending",
        "Travel & Places",
    ),
    e(
        "🕒",
        "clock",
        "Clock",
        "time schedule hour",
        "Travel & Places",
    ),
    // Tech, Dev & Objects
    e(
        "🔧",
        "wrench",
        "Wrench",
        "tool fix config settings",
        "Objects",
    ),
    e("🔨", "hammer", "Hammer", "build tool construct", "Objects"),
    e(
        "⚙️",
        "gear",
        "Gear",
        "settings config cog options",
        "Objects",
    ),
    e(
        "📦",
        "package",
        "Package",
        "box crate release bundle",
        "Objects",
    ),
    e(
        "🚢",
        "ship",
        "Ship",
        "deploy release boat",
        "Travel & Places",
    ),
    e("💻", "laptop", "Laptop", "computer code dev pc", "Objects"),
    e("⌨️", "keyboard", "Keyboard", "type input keys", "Objects"),
    e("📝", "memo", "Memo", "note write edit docs", "Objects"),
    e("📋", "clipboard", "Clipboard", "copy paste list", "Objects"),
    e("📁", "folder", "File Folder", "directory path", "Objects"),
    e("📄", "page", "Page Facing Up", "file document", "Objects"),
    e("🔖", "bookmark", "Bookmark", "save tag", "Objects"),
    e(
        "📊",
        "bar_chart",
        "Bar Chart",
        "stats metrics graph data",
        "Objects",
    ),
    e(
        "📅",
        "calendar",
        "Calendar",
        "date schedule event",
        "Objects",
    ),
    e("📧", "email", "E-Mail", "mail message inbox", "Objects"),
    e(
        "🏆",
        "trophy",
        "Trophy",
        "win award prize first",
        "Activities",
    ),
    e(
        "🥇",
        "first_place",
        "1st Place Medal",
        "gold winner",
        "Activities",
    ),
    e(
        "☕",
        "coffee",
        "Hot Beverage",
        "cafe tea cup break",
        "Food & Drink",
    ),
    e("🍕", "pizza", "Pizza", "food slice lunch", "Food & Drink"),
    e("🍺", "beer", "Beer Mug", "drink cheers", "Food & Drink"),
    // Nature & Weather
    e(
        "☀️",
        "sun",
        "Sun",
        "day bright light weather",
        "Nature & Weather",
    ),
    e(
        "🌙",
        "moon",
        "Crescent Moon",
        "night dark sleep",
        "Nature & Weather",
    ),
    e("☁️", "cloud", "Cloud", "weather sky", "Nature & Weather"),
    e(
        "🌧️",
        "rain",
        "Cloud with Rain",
        "weather storm wet",
        "Nature & Weather",
    ),
    e(
        "❄️",
        "snowflake",
        "Snowflake",
        "cold winter ice",
        "Nature & Weather",
    ),
    e(
        "🌈",
        "rainbow",
        "Rainbow",
        "colors pride",
        "Nature & Weather",
    ),
    e(
        "🌍",
        "globe",
        "Globe",
        "world earth web",
        "Nature & Weather",
    ),
    e(
        "🌲",
        "tree",
        "Evergreen Tree",
        "forest nature pine",
        "Nature & Weather",
    ),
    e(
        "🌱",
        "seedling",
        "Seedling",
        "plant grow sprout new",
        "Nature & Weather",
    ),
    // Arrows (UI & Developer Symbols)
    e(
        "→",
        "arrow_right",
        "Right Arrow",
        "arrow next forward goto",
        "Arrows",
    ),
    e("←", "arrow_left", "Left Arrow", "arrow back prev", "Arrows"),
    e("↑", "arrow_up", "Up Arrow", "arrow top north", "Arrows"),
    e(
        "↓",
        "arrow_down",
        "Down Arrow",
        "arrow bottom south",
        "Arrows",
    ),
    e(
        "↗",
        "arrow_up_right",
        "North East Arrow",
        "arrow external link open",
        "Arrows",
    ),
    e(
        "↘",
        "arrow_down_right",
        "South East Arrow",
        "arrow diagonal",
        "Arrows",
    ),
    e(
        "↙",
        "arrow_down_left",
        "South West Arrow",
        "arrow diagonal",
        "Arrows",
    ),
    e(
        "↖",
        "arrow_up_left",
        "North West Arrow",
        "arrow diagonal",
        "Arrows",
    ),
    e(
        "↔",
        "arrow_left_right",
        "Left Right Arrow",
        "arrow both horizontal swap",
        "Arrows",
    ),
    e(
        "↕",
        "arrow_up_down",
        "Up Down Arrow",
        "arrow vertical resize",
        "Arrows",
    ),
    e(
        "⇒",
        "double_arrow_right",
        "Rightwards Double Arrow",
        "arrow implies fat",
        "Arrows",
    ),
    e(
        "⇐",
        "double_arrow_left",
        "Leftwards Double Arrow",
        "arrow implied by",
        "Arrows",
    ),
    e(
        "⇔",
        "double_arrow_both",
        "Left Right Double Arrow",
        "arrow iff equivalent",
        "Arrows",
    ),
    e(
        "↵",
        "return_arrow",
        "Return Symbol",
        "arrow enter newline",
        "Arrows",
    ),
    e(
        "↻",
        "refresh_arrow",
        "Clockwise Open Circle Arrow",
        "arrow reload sync repeat",
        "Arrows",
    ),
    // Typography, UI & Keyboard Symbols
    e(
        "✓",
        "check",
        "Check Mark",
        "tick yes done pass",
        "UI Symbols",
    ),
    e(
        "✔",
        "heavy_check",
        "Heavy Check Mark",
        "tick yes done bold",
        "UI Symbols",
    ),
    e(
        "✕",
        "cross",
        "Multiplication X",
        "close cancel delete x",
        "UI Symbols",
    ),
    e(
        "✖",
        "heavy_cross",
        "Heavy Multiplication X",
        "close cancel error bold",
        "UI Symbols",
    ),
    e("•", "bullet", "Bullet", "dot list item point", "Typography"),
    e(
        "·",
        "middle_dot",
        "Middle Dot",
        "interpunct separator dot",
        "Typography",
    ),
    e(
        "…",
        "ellipsis",
        "Horizontal Ellipsis",
        "dots more continuation",
        "Typography",
    ),
    e(
        "—",
        "em_dash",
        "Em Dash",
        "dash hyphen punctuation",
        "Typography",
    ),
    e(
        "–",
        "en_dash",
        "En Dash",
        "dash range punctuation",
        "Typography",
    ),
    e(
        "©",
        "copyright",
        "Copyright Sign",
        "legal notice license",
        "Typography",
    ),
    e(
        "®",
        "registered",
        "Registered Sign",
        "trademark brand",
        "Typography",
    ),
    e(
        "™",
        "trademark",
        "Trade Mark Sign",
        "tm brand",
        "Typography",
    ),
    e(
        "°",
        "degree",
        "Degree Sign",
        "temp temperature angle celsius",
        "Typography",
    ),
    e(
        "§",
        "section",
        "Section Sign",
        "law paragraph",
        "Typography",
    ),
    e("¶", "pilcrow", "Pilcrow Sign", "paragraph", "Typography"),
    e(
        "⌘",
        "cmd",
        "Command Key",
        "mac apple shortcut keycap",
        "Keyboard",
    ),
    e("⌥", "option", "Option Key", "alt mac shortcut", "Keyboard"),
    e("⇧", "shift", "Shift Key", "arrow up keycap", "Keyboard"),
    e("⏎", "enter_key", "Return Key", "enter submit", "Keyboard"),
    e(
        "⌫",
        "backspace_key",
        "Erase to the Left",
        "backspace delete",
        "Keyboard",
    ),
    e("⇥", "tab_key", "Tab Key", "indent next", "Keyboard"),
    e(
        "⎋",
        "esc_key",
        "Broken Circle Arrow",
        "escape esc cancel",
        "Keyboard",
    ),
    // Math & Logic Operators
    e(
        "±",
        "plus_minus",
        "Plus-Minus Sign",
        "math tolerance approx",
        "Math",
    ),
    e(
        "×",
        "multiply",
        "Multiplication Sign",
        "math times cross product",
        "Math",
    ),
    e("÷", "divide", "Division Sign", "math slash", "Math"),
    e("≠", "not_equal", "Not Equal To", "math neq diff", "Math"),
    e(
        "≈",
        "approx",
        "Almost Equal To",
        "math approximately",
        "Math",
    ),
    e(
        "≤",
        "less_equal",
        "Less-Than or Equal To",
        "math leq",
        "Math",
    ),
    e(
        "≥",
        "greater_equal",
        "Greater-Than or Equal To",
        "math geq",
        "Math",
    ),
    e(
        "∞",
        "infinity",
        "Infinity",
        "math forever endless loop",
        "Math",
    ),
    e("∑", "sum", "N-Ary Summation", "math sigma total", "Math"),
    e("∏", "product", "N-Ary Product", "math pi", "Math"),
    e("√", "sqrt", "Square Root", "math radical", "Math"),
    e(
        "Δ",
        "delta",
        "Greek Capital Delta",
        "math diff change triangle",
        "Math",
    ),
    e("π", "pi", "Greek Small Pi", "math circle 3.14", "Math"),
    e(
        "λ",
        "lambda",
        "Greek Small Lambda",
        "math fn anonymous closure",
        "Math",
    ),
    e("μ", "mu", "Greek Small Mu", "math micro", "Math"),
    e("θ", "theta", "Greek Small Theta", "math angle", "Math"),
    e("σ", "sigma", "Greek Small Sigma", "math std", "Math"),
    e("Ω", "omega", "Greek Capital Omega", "math ohm big", "Math"),
    e("∀", "forall", "For All", "math logic universal", "Math"),
    e(
        "∃",
        "exists",
        "There Exists",
        "math logic existential",
        "Math",
    ),
    e("∈", "in_set", "Element Of", "math set member", "Math"),
    e("∴", "therefore", "Therefore", "math logic proof", "Math"),
    e("∵", "because", "Because", "math logic proof", "Math"),
    // Currency Symbols
    e(
        "$",
        "dollar",
        "Dollar Sign",
        "currency usd money",
        "Currency",
    ),
    e("€", "euro", "Euro Sign", "currency eur money", "Currency"),
    e(
        "£",
        "pound",
        "Pound Sign",
        "currency gbp sterling money",
        "Currency",
    ),
    e("¥", "yen", "Yen Sign", "currency jpy cny money", "Currency"),
    e(
        "₹",
        "rupee",
        "Indian Rupee Sign",
        "currency inr money",
        "Currency",
    ),
    e("₩", "won", "Won Sign", "currency krw money", "Currency"),
    e("¢", "cent", "Cent Sign", "currency coin money", "Currency"),
    e(
        "₿",
        "bitcoin",
        "Bitcoin Sign",
        "currency btc crypto",
        "Currency",
    ),
];

fn entry_for_id(id: &ItemId) -> Option<&'static EmojiEntry> {
    let slug = id.0.strip_prefix(ID_PREFIX)?;
    ENTRIES.iter().find(|entry| entry.slug == slug)
}

fn emoji_item(entry: &EmojiEntry) -> Item {
    Item {
        id: ItemId(format!("{ID_PREFIX}{}", entry.slug)),
        title: format!("{} {}", entry.glyph, entry.name),
        subtitle: format!("{} · {}", entry.category, entry.keywords),
        keywords: Vec::new(),
        alias: None,
        kind: ItemKind::Emoji,
        primary: ActionId("paste".into()),
        secondary: Some(ActionId("copy".into())),
    }
}

fn match_tier(entry: &EmojiEntry, query: &str, terms: &[&str]) -> Option<u8> {
    let name = entry.name.to_ascii_lowercase();
    let slug = entry.slug;
    let kw = entry.keywords;
    let cat = entry.category.to_ascii_lowercase();

    for &term in terms {
        let hit = name.contains(term)
            || slug.contains(term)
            || kw.contains(term)
            || cat.contains(term)
            || entry.glyph == term;
        if !hit {
            return None;
        }
    }

    if name == query || slug == query || entry.glyph == query {
        Some(0)
    } else if name.starts_with(query) || slug.starts_with(query) {
        Some(1)
    } else if name.contains(query) || kw.split_whitespace().any(|w| w == query) {
        Some(2)
    } else {
        Some(3)
    }
}

fn search_items(query: &str) -> Arc<[Item]> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return ENTRIES.iter().map(emoji_item).collect();
    }
    let terms: Vec<&str> = q.split_whitespace().collect();
    let mut ranked: Vec<(u8, usize, &EmojiEntry)> = ENTRIES
        .iter()
        .enumerate()
        .filter_map(|(idx, entry)| match_tier(entry, &q, &terms).map(|tier| (tier, idx, entry)))
        .collect();
    ranked.sort_by_key(|&(tier, idx, _)| (tier, idx));
    ranked
        .into_iter()
        .map(|(_, _, entry)| emoji_item(entry))
        .collect()
}

fn emoji_actions() -> Vec<Action> {
    vec![
        Action {
            id: ActionId("paste".into()),
            label: "Paste".into(),
            shortcut: None,
            enabled: Ok(()),
        },
        Action {
            id: ActionId("copy".into()),
            label: "Copy".into(),
            shortcut: None,
            enabled: Ok(()),
        },
    ]
}

fn run_emoji_action(id: &ItemId, action: &ActionId, cx: &mut Cx) -> Outcome {
    let Some(entry) = entry_for_id(id) else {
        return Outcome::Done;
    };
    match action.0.as_str() {
        "paste" => Outcome::Effects(vec![Effect::PasteText(entry.glyph.to_owned())]),
        "copy" => {
            cx.copy_text(entry.glyph.to_owned());
            Outcome::Done
        }
        _ => Outcome::Done,
    }
}

fn open_command_item() -> Item {
    Item {
        id: ItemId(OPEN_CMD_ID.into()),
        title: "Emoji & Symbols".into(),
        subtitle: String::new(),
        keywords: vec![
            "emoji".into(),
            "symbols".into(),
            "emoticon".into(),
            "unicode".into(),
        ],
        alias: None,
        kind: ItemKind::Command,
        primary: ActionId("run".into()),
        secondary: None,
    }
}

/// Root source: exposes the `Emoji & Symbols` command and claims `:` queries for inline lookup.
pub struct EmojiSource;

impl CommandSource for EmojiSource {
    fn claims(&self, q: &Query) -> bool {
        q.text.starts_with(':')
    }

    fn items(&self, q: &Query) -> Arc<[Item]> {
        match q.text.strip_prefix(':') {
            Some(rest) => search_items(rest),
            None => vec![open_command_item()].into(),
        }
    }

    fn actions(&self, id: &ItemId) -> Vec<Action> {
        if id.0 == OPEN_CMD_ID {
            vec![Action {
                id: ActionId("run".into()),
                label: "Open".into(),
                shortcut: None,
                enabled: Ok(()),
            }]
        } else {
            emoji_actions()
        }
    }

    fn run(&self, id: &ItemId, action: ActionId, cx: &mut Cx) -> Outcome {
        if id.0 == OPEN_CMD_ID {
            Outcome::Effects(vec![Effect::PushView(ViewId(VIEW_ID.into()))])
        } else {
            run_emoji_action(id, &action, cx)
        }
    }
}

/// Dedicated `emoji` View pushed by `Emoji & Symbols` or opened by optional `[hotkeys] emoji`.
pub struct EmojiView;

impl CommandSource for EmojiView {
    fn items(&self, q: &Query) -> Arc<[Item]> {
        search_items(&q.text)
    }

    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        emoji_actions()
    }

    fn run(&self, id: &ItemId, action: ActionId, cx: &mut Cx) -> Outcome {
        run_emoji_action(id, &action, cx)
    }
}

impl ViewSource for EmojiView {
    fn view_id(&self) -> ViewId {
        ViewId(VIEW_ID.into())
    }

    fn title(&self) -> &'static str {
        "Emoji & Symbols"
    }

    fn opened_by_hotkey(&self) -> bool {
        true
    }

    fn ranked(&self) -> bool {
        false
    }

    fn placeholder(&self) -> &'static str {
        "Search emojis and symbols..."
    }

    fn empty_title(&self) -> &'static str {
        "No matching emojis or symbols"
    }

    fn preview(&self, _id: &ItemId) -> Option<Preview> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn embedded_dataset_has_unique_slugs_and_non_empty_fields() {
        let mut slugs = HashSet::new();
        for entry in ENTRIES {
            assert!(!entry.glyph.is_empty(), "empty glyph for {}", entry.slug);
            assert!(!entry.slug.is_empty(), "empty slug");
            assert!(!entry.name.is_empty(), "empty name for {}", entry.slug);
            assert!(
                !entry.keywords.is_empty(),
                "empty keywords for {}",
                entry.slug
            );
            assert!(
                !entry.category.is_empty(),
                "empty category for {}",
                entry.slug
            );
            assert!(slugs.insert(entry.slug), "duplicate slug: {}", entry.slug);
        }
    }
}
