//! A WGSL syntax highlighter for iced.
use iced::advanced::text;
use iced_highlighter::Highlighter;
use iced_highlighter::Theme;

use once_cell::sync::Lazy;
use syntect::highlighting;
use syntect::parsing;

static WGSL_SYNTAX_SET: Lazy<parsing::SyntaxSet> = Lazy::new(|| {
    let mut builder = parsing::SyntaxSetBuilder::new();
    builder.add(
        parsing::SyntaxDefinition::load_from_str(
            include_str!("../../assets/WGSL.sublime-syntax"),
            true,
            None,
        )
        .unwrap(),
    );
    builder.build()
});
static THEMES: Lazy<highlighting::ThemeSet> = Lazy::new(highlighting::ThemeSet::load_defaults);

/// A syntax highlighter.
#[derive(Debug)]
pub struct WGSLHighlighter(Highlighter);

impl text::Highlighter for WGSLHighlighter {
    type Settings = <Highlighter as text::Highlighter>::Settings;
    type Highlight = <Highlighter as text::Highlighter>::Highlight;

    type Iterator<'a> = <Highlighter as text::Highlighter>::Iterator<'a>;

    fn new(settings: &Self::Settings) -> Self {
        let syntax = WGSL_SYNTAX_SET
            .find_syntax_by_token(&settings.token)
            .unwrap_or_else(|| WGSL_SYNTAX_SET.find_syntax_plain_text());

        let highlighter = highlighting::Highlighter::new(&THEMES.themes[key(settings.theme)]);
        let parser = parsing::ParseState::new(syntax);
        let stack = parsing::ScopeStack::new();

        WGSLHighlighter(Highlighter::new(
            syntax,
            &WGSL_SYNTAX_SET,
            highlighter,
            vec![(parser, stack)],
            0,
        ))
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.0.update(new_settings);
    }

    fn change_line(&mut self, line: usize) {
        self.0.change_line(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.0.highlight_line(line)
    }

    fn current_line(&self) -> usize {
        self.0.current_line()
    }
}
fn key(theme: Theme) -> &'static str {
    match theme {
        Theme::SolarizedDark => "Solarized (dark)",
        Theme::Base16Mocha => "base16-mocha.dark",
        Theme::Base16Ocean => "base16-ocean.dark",
        Theme::Base16Eighties => "base16-eighties.dark",
        Theme::InspiredGitHub => "InspiredGitHub",
    }
}
