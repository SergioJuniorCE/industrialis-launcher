//! Syntect grammars over the pinned native editor; the editing engine stays native.
use gpui::{Context, HighlightStyle, SharedString, Window};
use gpui_base::input::{
    EditorState, FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter,
    InputHighlighterFactory, Rope,
};
use std::{ops::Range, rc::Rc, sync::OnceLock};
use syntect::{
    easy::HighlightLines,
    highlighting::{HighlightState, Style, Theme, ThemeSet},
    parsing::{ParseState, SyntaxDefinition, SyntaxSet},
};

const FORGE: &str = r#"%YAML 1.2
---
name: Forge configuration
file_extensions: [cfg]
scope: source.forge
contexts:
  main:
    - match: '#.*$'
      scope: comment.line.number-sign
    - match: '\b(true|false)\b'
      scope: constant.language.boolean
    - match: '\b-?[0-9]+(\.[0-9]+)?\b'
      scope: constant.numeric
    - match: '^\s*([BISD])(:)("[^"]*"|[^=<>]+)'
      captures:
        1: storage.type
        2: punctuation.separator
        3: entity.name.tag
    - match: '"[^"\n]*"'
      scope: string.quoted.double
    - match: '[{}<>]'
      scope: punctuation.section.braces
"#;

fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(|| {
        let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
        builder.add(
            SyntaxDefinition::load_from_str(FORGE, true, None).expect("bundled Forge grammar"),
        );
        builder.build()
    })
}

fn themes() -> &'static ThemeSet {
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    THEMES.get_or_init(ThemeSet::load_defaults)
}

/// Load immutable grammar resources on the backend pool before first editing.
pub fn warm_up() {
    let _ = syntaxes();
    let _ = themes();
}

pub fn language_for_path(path: &str) -> String {
    let extension = path.rsplit('.').next().unwrap_or_default().to_lowercase();
    match extension.as_str() {
        "cfg" => "cfg",
        "properties" | "conf" | "ini" => "ini",
        "zs" => "java",
        "json5" => "json",
        other => other,
    }
    .to_string()
}

pub fn factory_for_theme(palette: &crate::theme::ActiveTheme) -> InputHighlighterFactory {
    let dark = palette.dark;
    let document = palette.editor_theme.clone();
    let background = palette.code_active_background();
    let base = palette.tokens.background.clone();
    let foreground = palette.tokens.foreground.clone();
    Rc::new(move |language| {
        let mut highlighter = SyntaxHighlighter::new(language, dark);
        highlighter.background = background;
        if let Some(document) = &document {
            highlighter.theme = textmate_theme(document, &base, &foreground);
        }
        Some(Box::new(highlighter))
    })
}

fn textmate_theme(document: &serde_json::Value, background: &str, foreground: &str) -> Theme {
    use syntect::highlighting::{Color, FontStyle, StyleModifier, ThemeItem};
    let color = |raw: &str| -> Option<Color> {
        let hex = industrialis_core::vscode_theme::opaque_color(raw, background)?;
        Some(Color {
            r: u8::from_str_radix(&hex[1..3], 16).ok()?,
            g: u8::from_str_radix(&hex[3..5], 16).ok()?,
            b: u8::from_str_radix(&hex[5..7], 16).ok()?,
            a: 255,
        })
    };
    let mut theme = Theme::default();
    theme.settings.foreground = color(foreground);
    theme.settings.background = color(background);
    for rule in document["tokenColors"].as_array().into_iter().flatten() {
        let scopes = if let Some(scope) = rule["scope"].as_str() {
            scope.to_owned()
        } else {
            rule["scope"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let settings = &rule["settings"];
        let foreground = settings["foreground"].as_str().and_then(color);
        if scopes.is_empty() {
            if foreground.is_some() {
                theme.settings.foreground = foreground;
            }
            continue;
        }
        let Ok(scope) = scopes.parse() else {
            continue;
        };
        let font_style = settings["fontStyle"].as_str().map(|styles| {
            let mut style = FontStyle::empty();
            for part in styles.split_whitespace() {
                match part {
                    "bold" => style |= FontStyle::BOLD,
                    "italic" => style |= FontStyle::ITALIC,
                    "underline" => style |= FontStyle::UNDERLINE,
                    _ => {}
                }
            }
            style
        });
        theme.scopes.push(ThemeItem {
            scope,
            style: StyleModifier {
                foreground,
                background: None,
                font_style,
            },
        });
    }
    theme
}

struct CachedLine {
    text: String,
    runs: Vec<(Range<usize>, Style)>,
    highlight: HighlightState,
    parse: ParseState,
}

struct SyntaxHighlighter {
    language: SharedString,
    theme: Theme,
    lines: Vec<CachedLine>,
    runs: Vec<(Range<usize>, HighlightStyle)>,
    background: gpui::Rgba,
}
impl SyntaxHighlighter {
    fn new(language: &str, dark: bool) -> Self {
        let themes = themes();
        Self {
            language: language.to_string().into(),
            theme: themes.themes[if dark {
                "base16-ocean.dark"
            } else {
                "InspiredGitHub"
            }]
            .clone(),
            lines: Vec::new(),
            runs: Vec::new(),
            background: crate::theme::editor_active_background(dark),
        }
    }

    fn rehighlight(&mut self, text: &str) {
        let set = syntaxes();
        let syntax = set
            .find_syntax_by_extension(&self.language)
            .unwrap_or_else(|| set.find_syntax_plain_text());
        let incoming: Vec<_> = text.split_inclusive('\n').collect();
        let prefix = self
            .lines
            .iter()
            .zip(&incoming)
            .take_while(|(old, new)| old.text == **new)
            .count();
        let mut highlighter = if prefix > 0 {
            let previous = &self.lines[prefix - 1];
            HighlightLines::from_state(
                &self.theme,
                previous.highlight.clone(),
                previous.parse.clone(),
            )
        } else {
            HighlightLines::new(syntax, &self.theme)
        };
        self.lines.truncate(prefix);
        for line in incoming.into_iter().skip(prefix) {
            let mut offset = 0;
            let runs = highlighter
                .highlight_line(line, set)
                .unwrap_or_default()
                .into_iter()
                .map(|(style, token)| {
                    let range = offset..offset + token.len();
                    offset = range.end;
                    (range, style)
                })
                .collect();
            let (highlight, parse) = highlighter.state();
            self.lines.push(CachedLine {
                text: line.to_string(),
                runs,
                highlight: highlight.clone(),
                parse: parse.clone(),
            });
            highlighter = HighlightLines::from_state(&self.theme, highlight, parse);
        }
        self.runs.clear();
        let mut offset = 0;
        for line in &self.lines {
            for (range, style) in &line.runs {
                // Keep syntax text readable over the product's editor surface.
                let color = gpui::Rgba {
                    r: f32::from(style.foreground.r) / 255.,
                    g: f32::from(style.foreground.g) / 255.,
                    b: f32::from(style.foreground.b) / 255.,
                    a: 1.,
                };
                let color = crate::theme::readable_color(color, self.background, 4.5);
                self.runs.push((
                    offset + range.start..offset + range.end,
                    HighlightStyle {
                        color: Some(color.into()),
                        font_weight: style
                            .font_style
                            .contains(syntect::highlighting::FontStyle::BOLD)
                            .then_some(gpui::FontWeight::BOLD),
                        font_style: style
                            .font_style
                            .contains(syntect::highlighting::FontStyle::ITALIC)
                            .then_some(gpui::FontStyle::Italic),
                        underline: style
                            .font_style
                            .contains(syntect::highlighting::FontStyle::UNDERLINE)
                            .then_some(gpui::UnderlineStyle {
                                thickness: gpui::px(1.),
                                color: Some(color.into()),
                                wavy: false,
                            }),
                        ..Default::default()
                    },
                ));
            }
            offset += line.text.len();
        }
    }
}

impl InputHighlighter for SyntaxHighlighter {
    fn language(&self) -> SharedString {
        self.language.clone()
    }
    fn update(
        &mut self,
        _: Option<InputEdit>,
        text: &Rope,
        _: bool,
        _: &mut Window,
        _: &mut Context<EditorState>,
    ) {
        self.rehighlight(&text.to_string());
    }
    fn styles(
        &self,
        range: &Range<usize>,
        _: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut runs = Vec::new();
        let mut cursor = range.start;
        for (span, style) in self
            .runs
            .iter()
            .filter(|(span, _)| span.end > range.start && span.start < range.end)
        {
            let start = span.start.max(range.start);
            let end = span.end.min(range.end);
            if cursor < start {
                runs.push((cursor..start, HighlightStyle::default()));
            }
            runs.push((start..end, *style));
            cursor = end;
        }
        if cursor < range.end {
            runs.push((cursor..range.end, HighlightStyle::default()));
        }
        runs
    }
    fn fold_ranges(&self, _: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_textmate_scopes_override_builtin_colors_and_font_styles() {
        let mut h = SyntaxHighlighter::new("cfg", true);
        h.background = gpui::rgb(0x101010);
        h.theme = textmate_theme(
            &serde_json::json!({"tokenColors":[
                {"scope":["comment","comment.line"],"settings":{"foreground":"#00ff00","fontStyle":"bold italic underline"}},
                {"scope":"string","settings":{"foreground":"#ff00ff"}}
            ]}),
            "#101010",
            "#eeeeee",
        );
        h.rehighlight("# comment\nS:name=\"hello\"\n");
        assert_eq!(h.runs[0].1.color, Some(gpui::rgb(0x00ff00).into()));
        assert_eq!(h.runs[0].1.font_weight, Some(gpui::FontWeight::BOLD));
        assert_eq!(h.runs[0].1.font_style, Some(gpui::FontStyle::Italic));
        assert!(h.runs[0].1.underline.is_some());
        assert!(h
            .runs
            .iter()
            .any(|(_, style)| style.color == Some(gpui::rgb(0xff00ff).into())));
    }
    #[test]
    fn forge_json_and_edits_highlight_unicode_without_touching_source() {
        for (language, source) in [
            (
                "cfg",
                "# comentário\ngeneral {\n B:enabled=true\n I:count=42\n S:name=\"中\"\n}\n",
            ),
            (
                "json",
                "{\"enabled\": true, \"name\": \"中\", \"count\": 42}\n",
            ),
        ] {
            for dark in [true, false] {
                let mut h = SyntaxHighlighter::new(language, dark);
                h.rehighlight(source);
                assert!(h.runs.len() > 5);
                assert!(h
                    .runs
                    .windows(2)
                    .any(|pair| pair[0].1.color != pair[1].1.color));
                assert_eq!(
                    h.lines
                        .iter()
                        .map(|line| line.text.as_str())
                        .collect::<String>(),
                    source
                );
                for (range, style) in &h.runs {
                    assert!(
                        source.is_char_boundary(range.start) && source.is_char_boundary(range.end)
                    );
                    let color: gpui::Rgba = style.color.unwrap().into();
                    assert!(
                        crate::theme::contrast_ratio(
                            color,
                            crate::theme::editor_active_background(dark)
                        ) >= 4.49
                    );
                    assert!(
                        crate::theme::contrast_ratio(color, crate::theme::editor_background(dark))
                            >= 4.49
                    );
                }
                let changed = source.replace("42", "123");
                h.rehighlight(&changed);
                let mut fresh = SyntaxHighlighter::new(language, dark);
                fresh.rehighlight(&changed);
                assert_eq!(h.runs, fresh.runs);
            }
        }
    }
}
