//! A small, deliberately incomplete — but **forgiving** — HTML tokenizer.
//!
//! This is **not** the [HTML5 tokenization state machine]. It recognises a tiny
//! grammar: doctype, comments, start/end tags, quoted/unquoted/valueless
//! attributes, and text. `<style>`/`<script>`/`<textarea>` use a minimal raw-text
//! mode — their body is captured verbatim until the matching close tag so their
//! contents are preserved — but this is not the full HTML raw-text/RCDATA
//! algorithm.
//!
//! Unlike earlier milestones, malformed input is now **recovered**, not rejected:
//! an unterminated tag/comment/attribute/raw-text block simply ends at EOF, an
//! unknown `<!` declaration is consumed as a bogus comment, and a stray `<` that
//! does not start a construct becomes literal text. The tokenizer therefore never
//! fails on real-world HTML; it only ever returns `Ok`. HTML character references
//! (`&amp;`, `&#160;`, `&#xA0;`, …) are decoded in text and attribute values
//! (but not inside raw-text elements, where `&` is literal).
//!
//! [HTML5 tokenization state machine]: https://html.spec.whatwg.org/multipage/parsing.html#tokenization

use mocha_dom::Attribute;
use mocha_error::MochaResult;

/// A single lexical token produced by [`tokenize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlToken {
    /// A `<!doctype ...>` declaration, storing the text after `doctype`.
    Doctype(String),
    /// A start tag such as `<div id="x">` or a self-closing `<div/>`.
    StartTag {
        /// Lowercased tag name.
        name: String,
        /// Attributes in source order.
        attributes: Vec<Attribute>,
        /// `true` if the tag ended with `/>`.
        self_closing: bool,
    },
    /// An end tag such as `</div>`.
    EndTag {
        /// Lowercased tag name.
        name: String,
    },
    /// A run of normalised text (trimmed, internal whitespace collapsed).
    Text(String),
    /// An HTML comment body (the text between `<!--` and `-->`).
    Comment(String),
}

/// Tokenize an HTML source string into a flat list of [`HtmlToken`]s.
///
/// Whitespace-only text runs collapse to a single space, and the whitespace
/// inside retained text runs is collapsed to single spaces and trimmed at the
/// ends. The function is infallible for any input — malformed markup is
/// recovered rather than rejected — but keeps the [`MochaResult`] return type so
/// callers do not need to change.
pub fn tokenize(input: &str) -> MochaResult<Vec<HtmlToken>> {
    Tokenizer::new(input).run()
}

struct Tokenizer {
    chars: Vec<char>,
    pos: usize,
}

impl Tokenizer {
    fn new(input: &str) -> Tokenizer {
        Tokenizer {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn run(mut self) -> MochaResult<Vec<HtmlToken>> {
        let mut tokens = Vec::new();
        while let Some(&c) = self.chars.get(self.pos) {
            if c == '<' {
                let token = self.read_markup();
                // `<style>`/`<script>`/`<textarea>` switch to a minimal raw-text
                // mode: the body is read verbatim until the matching close tag, so
                // contents containing `<`, `>`, or significant whitespace survive
                // rather than being tokenized as HTML. The close tag is then read
                // by the main loop. This is not the full HTML raw-text/RCDATA
                // algorithm.
                let raw_tag = match &token {
                    HtmlToken::StartTag {
                        name,
                        self_closing: false,
                        ..
                    } if is_raw_text_tag(name) => Some(name.clone()),
                    _ => None,
                };
                tokens.push(token);
                if let Some(tag) = raw_tag {
                    let raw = self.read_raw_text_until_close(&tag);
                    if !raw.is_empty() {
                        tokens.push(HtmlToken::Text(raw));
                    }
                }
            } else if let Some(text) = self.read_text() {
                tokens.push(HtmlToken::Text(text));
            }
        }
        Ok(tokens)
    }

    /// Read raw element content up to (but not consuming) the closing `</tag>`,
    /// matched case-insensitively. The text is returned verbatim — no whitespace
    /// collapsing and no entity decoding — so CSS/JS survive intact. If the close
    /// tag is never found, recovers by returning everything to EOF.
    fn read_raw_text_until_close(&mut self, tag: &str) -> String {
        let start = self.pos;
        while self.pos < self.chars.len() {
            if self.matches_close_tag(tag) {
                break;
            }
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    /// Whether the input at the current position is `</tag>` (or `</tag >`),
    /// matched case-insensitively.
    fn matches_close_tag(&self, tag: &str) -> bool {
        if self.chars.get(self.pos) != Some(&'<') || self.chars.get(self.pos + 1) != Some(&'/') {
            return false;
        }
        for (offset, expected) in tag.chars().enumerate() {
            match self.chars.get(self.pos + 2 + offset) {
                Some(actual) if actual.eq_ignore_ascii_case(&expected) => {}
                _ => return false,
            }
        }
        matches!(
            self.chars.get(self.pos + 2 + tag.len()),
            Some(c) if c.is_whitespace() || *c == '>'
        )
    }

    /// Read a run of text up to the next `<`, normalise its whitespace, and decode
    /// HTML character references.
    ///
    /// Returns `Some(" ")` for a whitespace-only run (so inter-tag whitespace
    /// separating inline content survives), and otherwise a normalised, entity-
    /// decoded string. Layout keeps edge whitespace invisible.
    fn read_text(&mut self) -> Option<String> {
        let start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            if c == '<' {
                break;
            }
            self.pos += 1;
        }
        let raw: String = self.chars[start..self.pos].iter().collect();
        normalize_text(&raw).map(|text| decode_entities(&text))
    }

    /// Read a markup construct that begins at the current `<`. A `<` that does not
    /// start a valid construct (e.g. `a < b`) is emitted as literal text.
    fn read_markup(&mut self) -> HtmlToken {
        match self.chars.get(self.pos + 1) {
            Some('!') => self.read_bang(),
            Some('/') => self.read_end_tag(),
            Some(c) if c.is_ascii_alphabetic() => self.read_start_tag(),
            _ => {
                // Not a tag/comment/doctype: treat the '<' as literal text.
                self.pos += 1;
                HtmlToken::Text("<".to_string())
            }
        }
    }

    /// Read a comment (`<!--`), a doctype (`<!doctype ...>`), or — for any other
    /// `<!` construct (CDATA, bogus declaration) — consume to `>` as a comment.
    fn read_bang(&mut self) -> HtmlToken {
        if self.starts_with("<!--") {
            return self.read_comment();
        }
        let rest: String = self.chars[self.pos..]
            .iter()
            .take(9)
            .collect::<String>()
            .to_ascii_lowercase();
        if rest == "<!doctype" {
            return self.read_doctype();
        }
        self.read_bogus_declaration()
    }

    /// Consume an unrecognised `<!...>` construct up to and including `>` (or EOF)
    /// and keep its body as a comment (comments produce no rendered box).
    fn read_bogus_declaration(&mut self) -> HtmlToken {
        self.pos += 2; // consume "<!"
        let start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            if c == '>' {
                break;
            }
            self.pos += 1;
        }
        let body: String = self.chars[start..self.pos].iter().collect();
        if self.chars.get(self.pos) == Some(&'>') {
            self.pos += 1;
        }
        HtmlToken::Comment(body.trim().to_string())
    }

    fn read_comment(&mut self) -> HtmlToken {
        self.pos += 4; // consume "<!--"
        let start = self.pos;
        while self.pos < self.chars.len() && !self.starts_with("-->") {
            self.pos += 1;
        }
        let body: String = self.chars[start..self.pos].iter().collect();
        if self.starts_with("-->") {
            self.pos += 3; // consume "-->"
        }
        // An unterminated comment simply ends at EOF (recovered, not an error).
        HtmlToken::Comment(body.trim().to_string())
    }

    fn read_doctype(&mut self) -> HtmlToken {
        self.pos += 9; // consume "<!doctype"
        let start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            if c == '>' {
                break;
            }
            self.pos += 1;
        }
        let body: String = self.chars[start..self.pos].iter().collect();
        if self.chars.get(self.pos) == Some(&'>') {
            self.pos += 1; // consume '>'
        }
        HtmlToken::Doctype(body.trim().to_string())
    }

    fn read_end_tag(&mut self) -> HtmlToken {
        self.pos += 2; // consume "</"
        let name = self.read_tag_name();
        // Consume up to and including '>' (recover past any junk like `</p foo>`).
        while let Some(&c) = self.chars.get(self.pos) {
            self.pos += 1;
            if c == '>' {
                break;
            }
        }
        HtmlToken::EndTag { name }
    }

    fn read_start_tag(&mut self) -> HtmlToken {
        self.pos += 1; // consume "<"
        let name = self.read_tag_name();
        let mut attributes = Vec::new();
        let mut self_closing = false;

        loop {
            self.skip_whitespace();
            match self.chars.get(self.pos) {
                // EOF mid-tag: recover by ending the tag here.
                None => break,
                Some('>') => {
                    self.pos += 1;
                    break;
                }
                Some('/') => {
                    if self.chars.get(self.pos + 1) == Some(&'>') {
                        self_closing = true;
                        self.pos += 2;
                        break;
                    }
                    // A stray '/' inside the tag: skip it and keep reading.
                    self.pos += 1;
                }
                // A new '<' before this tag closed: the tag was unterminated;
                // recover by ending it here without consuming the '<'.
                Some('<') => break,
                Some(_) => {
                    if let Some(attribute) = self.read_attribute() {
                        attributes.push(attribute);
                    }
                }
            }
        }

        HtmlToken::StartTag {
            name,
            attributes,
            self_closing,
        }
    }

    fn read_tag_name(&mut self) -> String {
        let start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            if c.is_ascii_alphanumeric() {
                self.pos += 1;
            } else {
                break;
            }
        }
        // An empty name (e.g. `</>`) is allowed; the tree builder ignores it.
        self.chars[start..self.pos]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase()
    }

    /// Read one attribute, returning `None` when there is nothing parseable (so
    /// the start-tag loop makes progress without producing junk attributes).
    fn read_attribute(&mut self) -> Option<Attribute> {
        // Attribute name: up to whitespace, '=', '>', '/', or '<'.
        let start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            if c.is_whitespace() || c == '=' || c == '>' || c == '/' || c == '<' {
                break;
            }
            self.pos += 1;
        }
        if self.pos == start {
            // The current char is one of `= / <` with no preceding name: skip it
            // so the caller's loop advances (recovery for input like `<div =x>`).
            self.pos += 1;
            return None;
        }
        let name: String = self.chars[start..self.pos]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();

        self.skip_whitespace();
        if self.chars.get(self.pos) != Some(&'=') {
            // Valueless attribute: value is the empty string.
            return Some(Attribute {
                name,
                value: String::new(),
            });
        }
        self.pos += 1; // consume '='
        self.skip_whitespace();
        let value = self.read_attribute_value();
        Some(Attribute {
            name,
            value: decode_entities(&value),
        })
    }

    fn read_attribute_value(&mut self) -> String {
        match self.chars.get(self.pos) {
            Some(&quote @ ('"' | '\'')) => {
                self.pos += 1; // consume opening quote
                let start = self.pos;
                while let Some(&c) = self.chars.get(self.pos) {
                    if c == quote {
                        break;
                    }
                    self.pos += 1;
                }
                let value: String = self.chars[start..self.pos].iter().collect();
                if self.chars.get(self.pos) == Some(&quote) {
                    self.pos += 1; // consume closing quote
                }
                // An unterminated quoted value simply ends at EOF (recovered).
                value
            }
            _ => {
                // Unquoted value: up to whitespace, '>', or '<'.
                let start = self.pos;
                while let Some(&c) = self.chars.get(self.pos) {
                    if c.is_whitespace() || c == '>' || c == '<' {
                        break;
                    }
                    self.pos += 1;
                }
                self.chars[start..self.pos].iter().collect()
            }
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(&c) = self.chars.get(self.pos) {
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn starts_with(&self, prefix: &str) -> bool {
        prefix
            .chars()
            .enumerate()
            .all(|(offset, c)| self.chars.get(self.pos + offset) == Some(&c))
    }
}

/// Whether a tag's body is parsed as raw text (its contents are not HTML).
/// `textarea`'s raw text becomes the control's initial value, so its whitespace
/// must survive verbatim (a simplification of HTML's RCDATA mode: no character
/// references are decoded).
fn is_raw_text_tag(name: &str) -> bool {
    matches!(name, "style" | "script" | "textarea")
}

/// Normalise an HTML text run: collapse internal whitespace to single spaces,
/// and preserve a single leading/trailing space when the run has content.
///
/// A whitespace-only run collapses to a single space `" "` (rather than being
/// dropped) so that inter-tag whitespace separating inline content survives.
fn normalize_text(text: &str) -> Option<String> {
    let core = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if core.is_empty() {
        return Some(" ".to_string());
    }
    let mut normalised = String::new();
    if text.starts_with(char::is_whitespace) {
        normalised.push(' ');
    }
    normalised.push_str(&core);
    if text.ends_with(char::is_whitespace) {
        normalised.push(' ');
    }
    Some(normalised)
}

/// Decode HTML character references in `input`. Recognises numeric references
/// (`&#160;`, `&#xA0;`) and a common named set; an unrecognised or unterminated
/// reference is left literal (the `&` and following text pass through unchanged).
fn decode_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '&' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        // Look for a terminating ';' within a small window after '&'.
        if let Some(semi) = (i + 1..(i + 32).min(chars.len())).find(|&j| chars[j] == ';') {
            if semi > i + 1 {
                let name: String = chars[i + 1..semi].iter().collect();
                if let Some(decoded) = decode_reference(&name) {
                    out.push(decoded);
                    i = semi + 1;
                    continue;
                }
            }
        }
        // Not a recognised reference: emit the '&' literally.
        out.push('&');
        i += 1;
    }
    out
}

/// Decode the body of a single reference (the text between `&` and `;`).
fn decode_reference(name: &str) -> Option<char> {
    if let Some(rest) = name.strip_prefix('#') {
        let code = if let Some(hex) = rest.strip_prefix(['x', 'X']) {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            rest.parse::<u32>().ok()?
        };
        return char::from_u32(code);
    }
    named_entity(name)
}

/// The common named character references. This is a pragmatic subset of the full
/// HTML named-character-reference table, covering what real content pages use.
fn named_entity(name: &str) -> Option<char> {
    let c = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "laquo" => '«',
        "raquo" => '»',
        "times" => '×',
        "divide" => '÷',
        "deg" => '°',
        "middot" => '·',
        "bull" => '•',
        "rarr" => '→',
        "larr" => '←',
        "dagger" => '†',
        "sect" => '§',
        "para" => '¶',
        "euro" => '€',
        "pound" => '£',
        "cent" => '¢',
        "yen" => '¥',
        "plusmn" => '±',
        "frac12" => '½',
        "frac14" => '¼',
        "frac34" => '¾',
        "micro" => 'µ',
        "agrave" => 'à',
        "aacute" => 'á',
        "acirc" => 'â',
        "atilde" => 'ã',
        "auml" => 'ä',
        "aring" => 'å',
        "ccedil" => 'ç',
        "egrave" => 'è',
        "eacute" => 'é',
        "ecirc" => 'ê',
        "euml" => 'ë',
        "iacute" => 'í',
        "ntilde" => 'ñ',
        "oacute" => 'ó',
        "ocirc" => 'ô',
        "ouml" => 'ö',
        "uacute" => 'ú',
        "uuml" => 'ü',
        "szlig" => 'ß',
        _ => return None,
    };
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_doctype() {
        let tokens = tokenize("<!doctype html>").unwrap();
        assert_eq!(tokens, vec![HtmlToken::Doctype("html".to_string())]);
    }

    #[test]
    fn tokenize_start_tag() {
        let tokens = tokenize("<div>").unwrap();
        assert_eq!(
            tokens,
            vec![HtmlToken::StartTag {
                name: "div".to_string(),
                attributes: Vec::new(),
                self_closing: false,
            }]
        );
    }

    #[test]
    fn tokenize_end_tag() {
        let tokens = tokenize("</div>").unwrap();
        assert_eq!(
            tokens,
            vec![HtmlToken::EndTag {
                name: "div".to_string()
            }]
        );
    }

    #[test]
    fn tokenize_text() {
        let tokens = tokenize("Hello   Mocha").unwrap();
        assert_eq!(tokens, vec![HtmlToken::Text("Hello Mocha".to_string())]);
    }

    #[test]
    fn leading_and_trailing_spaces_are_preserved_around_content() {
        assert_eq!(
            tokenize("Hello \n  world ").unwrap(),
            vec![HtmlToken::Text("Hello world ".into())]
        );
        assert_eq!(
            tokenize(" world").unwrap(),
            vec![HtmlToken::Text(" world".into())]
        );
    }

    #[test]
    fn whitespace_only_text_collapses_to_single_space() {
        let tokens = tokenize("<p>   </p>").unwrap();
        assert_eq!(
            tokens,
            vec![
                HtmlToken::StartTag {
                    name: "p".to_string(),
                    attributes: Vec::new(),
                    self_closing: false,
                },
                HtmlToken::Text(" ".to_string()),
                HtmlToken::EndTag {
                    name: "p".to_string()
                },
            ]
        );
    }

    #[test]
    fn whitespace_between_void_tags_is_preserved_as_one_space() {
        let tokens = tokenize("<input> <input>").unwrap();
        assert_eq!(
            tokens,
            vec![
                HtmlToken::StartTag {
                    name: "input".to_string(),
                    attributes: Vec::new(),
                    self_closing: false,
                },
                HtmlToken::Text(" ".to_string()),
                HtmlToken::StartTag {
                    name: "input".to_string(),
                    attributes: Vec::new(),
                    self_closing: false,
                },
            ]
        );
    }

    #[test]
    fn tokenize_comment() {
        let tokens = tokenize("<!-- hello -->").unwrap();
        assert_eq!(tokens, vec![HtmlToken::Comment("hello".to_string())]);
    }

    #[test]
    fn tokenize_quoted_attributes() {
        let tokens = tokenize(r#"<div id="main" class='box'>"#).unwrap();
        assert_eq!(
            tokens,
            vec![HtmlToken::StartTag {
                name: "div".to_string(),
                attributes: vec![
                    Attribute {
                        name: "id".to_string(),
                        value: "main".to_string()
                    },
                    Attribute {
                        name: "class".to_string(),
                        value: "box".to_string()
                    },
                ],
                self_closing: false,
            }]
        );
    }

    #[test]
    fn tokenize_valueless_attribute_is_empty_string() {
        let tokens = tokenize("<div hidden>").unwrap();
        match &tokens[0] {
            HtmlToken::StartTag { attributes, .. } => {
                assert_eq!(attributes[0].name, "hidden");
                assert_eq!(attributes[0].value, "");
            }
            other => panic!("expected start tag, got {other:?}"),
        }
    }

    #[test]
    fn tokenize_self_closing_tag() {
        let tokens = tokenize("<div/>").unwrap();
        match &tokens[0] {
            HtmlToken::StartTag { self_closing, .. } => assert!(self_closing),
            other => panic!("expected start tag, got {other:?}"),
        }
    }

    // --- recovery (Milestone 23): malformed input no longer errors -----------

    #[test]
    fn unterminated_comment_recovers_at_eof() {
        let tokens = tokenize("<!-- oops").unwrap();
        assert_eq!(tokens, vec![HtmlToken::Comment("oops".to_string())]);
    }

    #[test]
    fn unterminated_tag_recovers_at_eof() {
        let tokens = tokenize("<div").unwrap();
        assert_eq!(
            tokens,
            vec![HtmlToken::StartTag {
                name: "div".to_string(),
                attributes: Vec::new(),
                self_closing: false,
            }]
        );
    }

    #[test]
    fn unterminated_style_recovers_at_eof() {
        // No `</style>`: the body is captured to EOF and no error is produced.
        let tokens = tokenize("<style>p { color: red; }").unwrap();
        assert_eq!(
            tokens,
            vec![
                HtmlToken::StartTag {
                    name: "style".into(),
                    attributes: Vec::new(),
                    self_closing: false,
                },
                HtmlToken::Text("p { color: red; }".into()),
            ]
        );
    }

    #[test]
    fn stray_left_angle_bracket_becomes_text() {
        // `a < b` — the `<` is not a tag start, so it is literal text.
        let tokens = tokenize("a < b").unwrap();
        assert_eq!(
            tokens,
            vec![
                HtmlToken::Text("a ".into()),
                HtmlToken::Text("<".into()),
                HtmlToken::Text(" b".into()),
            ]
        );
    }

    #[test]
    fn unknown_bang_declaration_is_consumed_as_comment() {
        let tokens = tokenize("<![CDATA[x]]>after").unwrap();
        assert_eq!(
            tokens,
            vec![
                HtmlToken::Comment("[CDATA[x]]".into()),
                HtmlToken::Text("after".into()),
            ]
        );
    }

    #[test]
    fn numeric_and_named_entities_decode_in_text() {
        let tokens = tokenize("A&amp;B &#169; &#x41; &nbsp;end").unwrap();
        assert_eq!(tokens, vec![HtmlToken::Text("A&B © A \u{a0}end".into())]);
    }

    #[test]
    fn common_named_entities_decode() {
        let tokens =
            tokenize("a&mdash;b&ndash;c&hellip;&rarr;&larr;&times;&divide;&laquo;&raquo;").unwrap();
        assert_eq!(tokens, vec![HtmlToken::Text("a—b–c…→←×÷«»".into())]);
        let tokens = tokenize("&dagger;&bull;&deg;&para;&sect;&middot;&euro;&pound;&yen;").unwrap();
        assert_eq!(tokens, vec![HtmlToken::Text("†•°¶§·€£¥".into())]);
        let tokens = tokenize("&ldquo;&rdquo;&lsquo;&rsquo;").unwrap();
        assert_eq!(tokens, vec![HtmlToken::Text("“”‘’".into())]);
    }

    #[test]
    fn entities_decode_in_attribute_values() {
        let tokens = tokenize(r#"<a href="?a=1&amp;b=2">"#).unwrap();
        match &tokens[0] {
            HtmlToken::StartTag { attributes, .. } => {
                assert_eq!(attributes[0].value, "?a=1&b=2");
            }
            other => panic!("expected start tag, got {other:?}"),
        }
    }

    #[test]
    fn unknown_entity_is_left_literal() {
        let tokens = tokenize("x &notareal; y").unwrap();
        assert_eq!(tokens, vec![HtmlToken::Text("x &notareal; y".into())]);
    }

    #[test]
    fn entities_are_not_decoded_inside_raw_text() {
        let tokens = tokenize("<script>if (a &amp;&amp; b) {}</script>").unwrap();
        assert_eq!(tokens[1], HtmlToken::Text("if (a &amp;&amp; b) {}".into()));
    }

    #[test]
    fn style_body_is_raw_text_and_not_tokenized_as_markup() {
        let tokens = tokenize(r#"<style>/* <not-a-tag> */ p { color: red; }</style>"#).unwrap();
        assert_eq!(
            tokens,
            vec![
                HtmlToken::StartTag {
                    name: "style".into(),
                    attributes: Vec::new(),
                    self_closing: false,
                },
                HtmlToken::Text("/* <not-a-tag> */ p { color: red; }".into()),
                HtmlToken::EndTag {
                    name: "style".into()
                },
            ]
        );
    }

    #[test]
    fn style_body_preserves_whitespace_verbatim() {
        let tokens = tokenize("<style>  p  {  }  </style>").unwrap();
        assert_eq!(tokens[1], HtmlToken::Text("  p  {  }  ".into()));
    }

    #[test]
    fn empty_style_produces_no_text_token() {
        let tokens = tokenize("<style></style>").unwrap();
        assert_eq!(
            tokens,
            vec![
                HtmlToken::StartTag {
                    name: "style".into(),
                    attributes: Vec::new(),
                    self_closing: false,
                },
                HtmlToken::EndTag {
                    name: "style".into()
                },
            ]
        );
    }
}
