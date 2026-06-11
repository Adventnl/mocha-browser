//! A small, deliberately incomplete HTML tokenizer.
//!
//! This is **not** the [HTML5 tokenization state machine]. It recognises a tiny
//! grammar: doctype, comments, start/end tags, quoted/unquoted/valueless
//! attributes, and text. `<style>` uses a minimal raw-text mode — its body is
//! captured verbatim until `</style>` so CSS containing `<`/`>` is preserved —
//! but this is not the full HTML raw-text/RCDATA algorithm. Anything malformed
//! (an unterminated tag, comment, attribute quote, or `<style>`) is reported as
//! a [`MochaError::Parse`] error instead of being silently recovered.
//!
//! [HTML5 tokenization state machine]: https://html.spec.whatwg.org/multipage/parsing.html#tokenization

use mocha_dom::Attribute;
use mocha_error::{MochaError, MochaResult};

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
/// Whitespace-only text runs are dropped, and the whitespace inside retained
/// text runs is collapsed to single spaces and trimmed at the ends.
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
                let token = self.read_markup()?;
                // `<style>` and `<script>` switch to a minimal raw-text mode: the
                // body is read verbatim until the matching close tag, so CSS or JS
                // containing `<`, `>`, or significant whitespace is preserved
                // rather than being tokenized as HTML. The close tag is then read
                // by the main loop. This is not the full HTML raw-text/RCDATA
                // algorithm (no `</style` / `</script` escaping subtleties).
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
                    let raw = self.read_raw_text_until_close(&tag)?;
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

    /// Read raw element content up to (but not consuming) the closing
    /// `</tag>`, matched case-insensitively. The text is returned verbatim — no
    /// whitespace collapsing — so CSS survives intact. Returns a [`MochaError::Parse`]
    /// error if the closing tag is never found.
    fn read_raw_text_until_close(&mut self, tag: &str) -> MochaResult<String> {
        let start = self.pos;
        while self.pos < self.chars.len() {
            if self.matches_close_tag(tag) {
                return Ok(self.chars[start..self.pos].iter().collect());
            }
            self.pos += 1;
        }
        Err(MochaError::Parse(format!(
            "unterminated <{tag}>: missing </{tag}>"
        )))
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

    /// Read a run of text up to the next `<` and normalise its whitespace.
    ///
    /// Returns `None` when the run is whitespace-only (so block-level whitespace
    /// between tags disappears). For runs with content, internal whitespace is
    /// collapsed to single spaces and a single leading/trailing space is
    /// preserved when present — this keeps the spaces around inline elements
    /// (e.g. `Hello <span>red</span> world`) intact for inline layout.
    fn read_text(&mut self) -> Option<String> {
        let start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            if c == '<' {
                break;
            }
            self.pos += 1;
        }
        let raw: String = self.chars[start..self.pos].iter().collect();
        normalize_text(&raw)
    }

    /// Read a markup construct that begins at the current `<`.
    fn read_markup(&mut self) -> MochaResult<HtmlToken> {
        // Look past the '<' to decide which construct this is.
        match self.chars.get(self.pos + 1) {
            Some('!') => self.read_bang(),
            Some('/') => self.read_end_tag(),
            Some(c) if c.is_ascii_alphabetic() => self.read_start_tag(),
            _ => Err(MochaError::Parse(format!(
                "unexpected character after '<' at position {}",
                self.pos
            ))),
        }
    }

    /// Read either a comment (`<!--`) or a doctype (`<!doctype ...>`).
    fn read_bang(&mut self) -> MochaResult<HtmlToken> {
        if self.starts_with("<!--") {
            return self.read_comment();
        }
        // Case-insensitively match `<!doctype`.
        let rest: String = self.chars[self.pos..]
            .iter()
            .take(9)
            .collect::<String>()
            .to_ascii_lowercase();
        if rest == "<!doctype" {
            return self.read_doctype();
        }
        Err(MochaError::Parse(format!(
            "unsupported '<!' declaration at position {}",
            self.pos
        )))
    }

    fn read_comment(&mut self) -> MochaResult<HtmlToken> {
        self.pos += 4; // consume "<!--"
        let start = self.pos;
        while self.pos < self.chars.len() && !self.starts_with("-->") {
            self.pos += 1;
        }
        if self.pos >= self.chars.len() {
            return Err(MochaError::Parse(
                "unterminated comment: missing '-->'".to_string(),
            ));
        }
        let body: String = self.chars[start..self.pos].iter().collect();
        self.pos += 3; // consume "-->"
        Ok(HtmlToken::Comment(body.trim().to_string()))
    }

    fn read_doctype(&mut self) -> MochaResult<HtmlToken> {
        self.pos += 9; // consume "<!doctype"
        let start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            if c == '>' {
                break;
            }
            self.pos += 1;
        }
        if self.chars.get(self.pos) != Some(&'>') {
            return Err(MochaError::Parse(
                "unterminated doctype: missing '>'".to_string(),
            ));
        }
        let body: String = self.chars[start..self.pos].iter().collect();
        self.pos += 1; // consume '>'
        Ok(HtmlToken::Doctype(body.trim().to_string()))
    }

    fn read_end_tag(&mut self) -> MochaResult<HtmlToken> {
        self.pos += 2; // consume "</"
        let name = self.read_tag_name()?;
        self.skip_whitespace();
        if self.chars.get(self.pos) != Some(&'>') {
            return Err(MochaError::Parse(format!(
                "malformed end tag </{name}>: missing '>'"
            )));
        }
        self.pos += 1; // consume '>'
        Ok(HtmlToken::EndTag { name })
    }

    fn read_start_tag(&mut self) -> MochaResult<HtmlToken> {
        self.pos += 1; // consume "<"
        let name = self.read_tag_name()?;
        let mut attributes = Vec::new();
        let mut self_closing = false;

        loop {
            self.skip_whitespace();
            match self.chars.get(self.pos) {
                None => {
                    return Err(MochaError::Parse(format!(
                        "unterminated start tag <{name}>: missing '>'"
                    )));
                }
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
                    return Err(MochaError::Parse(format!(
                        "unexpected '/' inside start tag <{name}>"
                    )));
                }
                Some(_) => attributes.push(self.read_attribute()?),
            }
        }

        Ok(HtmlToken::StartTag {
            name,
            attributes,
            self_closing,
        })
    }

    fn read_tag_name(&mut self) -> MochaResult<String> {
        let start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            if c.is_ascii_alphanumeric() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(MochaError::Parse(format!(
                "expected a tag name at position {}",
                self.pos
            )));
        }
        let name: String = self.chars[start..self.pos].iter().collect();
        Ok(name.to_ascii_lowercase())
    }

    fn read_attribute(&mut self) -> MochaResult<Attribute> {
        // Attribute name: up to whitespace, '=', '>', or '/'.
        let start = self.pos;
        while let Some(&c) = self.chars.get(self.pos) {
            if c.is_whitespace() || c == '=' || c == '>' || c == '/' {
                break;
            }
            self.pos += 1;
        }
        if self.pos == start {
            return Err(MochaError::Parse(format!(
                "expected an attribute name at position {}",
                self.pos
            )));
        }
        let name: String = self.chars[start..self.pos]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();

        self.skip_whitespace();
        if self.chars.get(self.pos) != Some(&'=') {
            // Valueless attribute: value is the empty string.
            return Ok(Attribute {
                name,
                value: String::new(),
            });
        }
        self.pos += 1; // consume '='
        self.skip_whitespace();
        let value = self.read_attribute_value()?;
        Ok(Attribute { name, value })
    }

    fn read_attribute_value(&mut self) -> MochaResult<String> {
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
                if self.chars.get(self.pos) != Some(&quote) {
                    return Err(MochaError::Parse(
                        "unterminated quoted attribute value".to_string(),
                    ));
                }
                let value: String = self.chars[start..self.pos].iter().collect();
                self.pos += 1; // consume closing quote
                Ok(value)
            }
            _ => {
                // Unquoted value: up to whitespace or '>'.
                let start = self.pos;
                while let Some(&c) = self.chars.get(self.pos) {
                    if c.is_whitespace() || c == '>' {
                        break;
                    }
                    self.pos += 1;
                }
                if self.pos == start {
                    return Err(MochaError::Parse(
                        "expected an attribute value after '='".to_string(),
                    ));
                }
                Ok(self.chars[start..self.pos].iter().collect())
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
/// must survive verbatim (this is a simplification of HTML's RCDATA mode: no
/// character references are decoded).
fn is_raw_text_tag(name: &str) -> bool {
    matches!(name, "style" | "script" | "textarea")
}

/// Normalise an HTML text run: collapse internal whitespace to single spaces,
/// and preserve a single leading/trailing space when the run has content.
/// Returns `None` for a whitespace-only run.
fn normalize_text(text: &str) -> Option<String> {
    let core = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if core.is_empty() {
        return None;
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
        // Spaces adjacent to inline elements must survive (collapsed to one).
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
    fn whitespace_only_text_is_dropped() {
        let tokens = tokenize("<p>   </p>").unwrap();
        assert_eq!(
            tokens,
            vec![
                HtmlToken::StartTag {
                    name: "p".to_string(),
                    attributes: Vec::new(),
                    self_closing: false,
                },
                HtmlToken::EndTag {
                    name: "p".to_string()
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

    #[test]
    fn unterminated_comment_is_a_parse_error() {
        let error = tokenize("<!-- oops").unwrap_err();
        assert!(matches!(error, MochaError::Parse(_)));
    }

    #[test]
    fn unterminated_tag_is_a_parse_error() {
        let error = tokenize("<div").unwrap_err();
        assert!(matches!(error, MochaError::Parse(_)));
    }

    #[test]
    fn style_body_is_raw_text_and_not_tokenized_as_markup() {
        // The `<` inside the CSS must not start an HTML tag.
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
    fn unterminated_style_is_a_parse_error() {
        let error = tokenize("<style>p { color: red; }").unwrap_err();
        match error {
            MochaError::Parse(message) => assert!(message.contains("</style>")),
            other => panic!("expected Parse error, got {other:?}"),
        }
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
