//! Preview texts for fonts (the Fonts page's preview, the tournament), as
//! Pango markup in the theme's colours: code in common languages, a Nerd
//! Font icon sample, prose (chapter 1 of Frankenstein), and a README-style
//! document.

use crate::palette::Palette;

/// What a preview shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Code,
    Icons,
    Prose,
    Readme,
}

pub const KINDS: [(Kind, &str); 4] = [
    (Kind::Code, "code"),
    (Kind::Icons, "icons"),
    (Kind::Prose, "prose"),
    (Kind::Readme, "README"),
];

/// (name, keywords, line comment, code). The code is in `samples/code/`,
/// realistic files of 40–60 lines.
pub const LANGS: [(&str, &[&str], &str, &str); 13] = [
    (
        "Rust",
        &[
            "fn", "let", "mut", "if", "else", "match", "for", "in", "return", "pub", "struct",
            "enum", "impl", "use", "Some", "None", "Ok", "Err", "self", "Self", "true", "false",
            "continue", "as",
        ],
        "//",
        include_str!("samples/code/rust.rs"),
    ),
    (
        "Python",
        &[
            "def", "class", "return", "if", "elif", "else", "for", "in", "import", "from", "with",
            "as", "None", "True", "False", "and", "or", "not", "self", "try", "except", "raise",
            "pass", "lambda",
        ],
        "#",
        include_str!("samples/code/python.py"),
    ),
    (
        "TypeScript",
        &[
            "const",
            "let",
            "function",
            "return",
            "if",
            "else",
            "for",
            "of",
            "interface",
            "type",
            "export",
            "import",
            "from",
            "async",
            "await",
            "new",
            "true",
            "false",
            "null",
            "class",
            "extends",
            "throw",
            "try",
            "catch",
            "public",
            "void",
            "as",
        ],
        "//",
        include_str!("samples/code/typescript.ts"),
    ),
    (
        "Go",
        &[
            "package", "import", "func", "return", "if", "else", "for", "range", "var", "const",
            "type", "struct", "nil", "true", "false", "defer", "go", "err", "map",
        ],
        "//",
        include_str!("samples/code/go.go"),
    ),
    (
        "C",
        &[
            "int", "char", "void", "return", "if", "else", "for", "while", "struct", "static",
            "const", "unsigned", "sizeof", "include", "define", "NULL", "double", "long",
        ],
        "//",
        include_str!("samples/code/c.c"),
    ),
    (
        "Lua",
        &[
            "local", "function", "return", "if", "then", "else", "elseif", "end", "for", "in",
            "do", "while", "nil", "true", "false", "and", "or", "not",
        ],
        "--",
        include_str!("samples/code/lua.lua"),
    ),
    (
        "shell",
        &[
            "if", "then", "else", "fi", "for", "in", "do", "done", "while", "case", "esac",
            "function", "local", "return", "export", "set", "exit", "shift",
        ],
        "#",
        include_str!("samples/code/shell.sh"),
    ),
    (
        "Java",
        &[
            "package", "import", "public", "private", "final", "class", "enum", "record", "static",
            "void", "return", "new", "if", "throw", "throws", "try", "catch", "var", "this", "int",
            "null", "true", "false",
        ],
        "//",
        include_str!("samples/code/java.java"),
    ),
    (
        "Kotlin",
        &[
            "package",
            "import",
            "class",
            "data",
            "sealed",
            "interface",
            "val",
            "var",
            "fun",
            "return",
            "if",
            "else",
            "when",
            "is",
            "in",
            "init",
            "private",
            "get",
            "while",
            "null",
            "true",
            "false",
            "it",
        ],
        "//",
        include_str!("samples/code/kotlin.kt"),
    ),
    (
        "Ruby",
        &[
            "require",
            "class",
            "def",
            "end",
            "if",
            "unless",
            "else",
            "do",
            "return",
            "include",
            "attr_reader",
            "raise",
            "rescue",
            "nil",
            "true",
            "false",
            "self",
            "new",
            "or",
        ],
        "#",
        include_str!("samples/code/ruby.rb"),
    ),
    (
        "Haskell",
        &[
            "module",
            "where",
            "import",
            "qualified",
            "as",
            "data",
            "newtype",
            "deriving",
            "let",
            "in",
            "if",
            "then",
            "else",
            "do",
            "otherwise",
        ],
        "--",
        include_str!("samples/code/haskell.hs"),
    ),
    (
        "SQL",
        &[
            "CREATE",
            "TABLE",
            "IF",
            "NOT",
            "EXISTS",
            "INTEGER",
            "TEXT",
            "TIMESTAMP",
            "PRIMARY",
            "KEY",
            "NULL",
            "UNIQUE",
            "CHECK",
            "IN",
            "DEFAULT",
            "REFERENCES",
            "ON",
            "DELETE",
            "CASCADE",
            "INDEX",
            "INSERT",
            "INTO",
            "VALUES",
            "CONFLICT",
            "DO",
            "NOTHING",
            "WITH",
            "AS",
            "SELECT",
            "FROM",
            "JOIN",
            "LEFT",
            "WHERE",
            "AND",
            "IS",
            "GROUP",
            "BY",
            "ORDER",
            "DESC",
            "LIMIT",
            "OVER",
            "BETWEEN",
        ],
        "--",
        include_str!("samples/code/sql.sql"),
    ),
    (
        "HTML/CSS",
        &[
            "DOCTYPE", "html", "head", "body", "meta", "title", "style", "header", "main",
            "section", "div", "p", "h1", "code", "pre", "button", "a", "media",
        ],
        "<!--",
        include_str!("samples/code/html.html"),
    ),
];

/// The paragraphs of chapter 1 (first: the title line).
pub fn prose() -> Vec<&'static str> {
    include_str!("samples/frankenstein-1.txt")
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The fonts and colours a preview is drawn with.
pub struct Style<'a> {
    pub family: &'a str,
    /// Points.
    pub size: u32,
    pub palette: &'a Palette,
}

impl Style<'_> {
    fn c(&self, k: &str) -> String {
        self.palette.get(k).hex()
    }

    fn open(&self) -> String {
        format!(
            "<span font=\"{} {}\" foreground=\"{}\">",
            esc(self.family),
            self.size,
            self.c("fg0")
        )
    }
}

/// How many lines a language's sample has.
pub fn code_lines(lang: usize) -> usize {
    LANGS[lang % LANGS.len()].3.lines().count()
}

/// Code from line `first` on, highlighted: keywords, strings, numbers,
/// comments, calls, types.
pub fn code(st: &Style, lang: usize, first: usize) -> String {
    let (_, keywords, comment, src) = LANGS[lang % LANGS.len()];
    let span = |k: &str, t: &str| format!("<span foreground=\"{}\">{}</span>", st.c(k), esc(t));
    let mut out = st.open();
    for line in src.lines().skip(first) {
        let mut rest = line;
        while !rest.is_empty() {
            let c = rest.chars().next().unwrap();
            if rest.starts_with(comment) || rest.starts_with("/*") || rest.starts_with("/**") {
                out += &format!(
                    "<span foreground=\"{}\"><i>{}</i></span>",
                    st.c("grey1"),
                    esc(rest)
                );
                break;
            }
            if c == '"' || c == '\'' || c == '`' {
                let end = rest[1..].find(c).map_or(rest.len(), |i| i + 2);
                out += &span("green", &rest[..end]);
                rest = &rest[end..];
            } else if c.is_ascii_digit() {
                let end = rest
                    .find(|x: char| !(x.is_ascii_alphanumeric() || x == '.'))
                    .unwrap_or(rest.len());
                out += &span("orange", &rest[..end]);
                rest = &rest[end..];
            } else if c.is_alphabetic() || c == '_' || c == '#' {
                let end = rest[c.len_utf8()..]
                    .find(|x: char| !(x.is_alphanumeric() || x == '_'))
                    .map_or(rest.len(), |i| i + c.len_utf8());
                let word = &rest[..end];
                let bare = word.trim_start_matches('#');
                let after = &rest[end..];
                out += &if keywords.contains(&bare) {
                    span("purple", word)
                } else if after.starts_with('(') || after.starts_with("!(") {
                    span("blue", word)
                } else if bare.chars().next().is_some_and(char::is_uppercase) {
                    span("yellow", word)
                } else {
                    esc(word)
                };
                rest = after;
            } else if "{}[]()<>=+-*/&|!;:,.%".contains(c) {
                out += &span("grey2", &rest[..c.len_utf8()]);
                rest = &rest[c.len_utf8()..];
            } else {
                out += &esc(&rest[..c.len_utf8()]);
                rest = &rest[c.len_utf8()..];
            }
        }
        out.push('\n');
    }
    out + "</span>"
}

/// Nerd Font icons in their usual places: a prompt, a file tree, git
/// status, a status bar.
pub fn icons(st: &Style) -> String {
    let span = |k: &str, t: &str| format!("<span foreground=\"{}\">{}</span>", st.c(k), esc(t));
    let mut out = st.open();
    // a prompt
    out += &format!(
        "{}{}{} {}\n\n",
        format_args!(
            "<span background=\"{}\" foreground=\"{}\"> \u{f17c} ~/dotfiles </span>",
            st.c("blue"),
            st.c("bg0")
        ),
        span("blue", "\u{e0b0}"),
        span("green", " \u{e0a0} main"),
        span("yellow", "\u{f00c} \u{f071} 2")
    );
    // a file tree
    for (icon, colour, name) in [
        ("\u{f07c}", "blue", "src/"),
        ("\u{e7a8}", "orange", "  main.rs"),
        ("\u{e606}", "yellow", "  build.py"),
        ("\u{e628}", "blue", "  app.ts"),
        ("\u{e627}", "aqua", "  server.go"),
        ("\u{e620}", "blue", "  init.lua"),
        ("\u{f313}", "blue", "flake.nix"),
        ("\u{e60b}", "yellow", "package.json"),
        ("\u{e73e}", "fg0", "README.md"),
        ("\u{e702}", "red", ".gitignore"),
    ] {
        let indent = name.len() - name.trim_start().len();
        out += &format!(
            "{}{} {}\n",
            " ".repeat(indent),
            span(colour, icon),
            esc(name.trim_start())
        );
    }
    // git status
    out += &format!(
        "\n{} 3  {} 1  {} 2  {} main ↑1 ↓0\n",
        span("green", "\u{f067}"),
        span("red", "\u{f00d}"),
        span("yellow", "\u{f040}"),
        span("purple", "\u{e725}")
    );
    // a status bar
    out += &format!(
        "\n{}  {} 73%  {} wifi  {} 82%  {} 4.1G  {} 18:41\n",
        span("blue", "\u{f4bc} 12%"),
        span("aqua", "\u{f028}"),
        span("green", "\u{f1eb}"),
        span("yellow", "\u{f240}"),
        span("purple", "\u{f0c9}"),
        span("fg0", "\u{f017}")
    );
    out + "</span>"
}

/// Prose from paragraph `from`, about `budget` characters' worth; returns
/// the markup and where the next page starts.
pub fn prose_page(st: &Style, from: usize, budget: usize) -> (String, usize) {
    let paras = prose();
    let from = from.min(paras.len().saturating_sub(1));
    let mut out = st.open();
    let mut used = 0;
    let mut i = from;
    while i < paras.len() && (used == 0 || used + paras[i].len() <= budget) {
        let p = paras[i];
        out += &if i == 0 {
            format!(
                "<span foreground=\"{}\"><i>{}</i></span>\n\n",
                st.c("grey1"),
                esc(p)
            )
        } else if p.starts_with("Chapter") {
            format!(
                "<span size=\"{}\"><b>{}</b></span>\n\n",
                st.size * 1400,
                esc(p)
            )
        } else {
            format!("{}\n\n", esc(p))
        };
        used += p.len();
        i += 1;
    }
    (out + "</span>", if i >= paras.len() { from } else { i })
}

/// A README-style document: headings, a list, inline code, a code block, a
/// table and a quote.
pub fn readme(st: &Style) -> String {
    let big = |scale: u32, t: &str| {
        format!(
            "<span size=\"{}\"><b>{}</b></span>\n",
            st.size * scale,
            esc(t)
        )
    };
    let code = |t: &str| format!("<span background=\"{}\">{}</span>", st.c("bg2"), esc(t));
    let mut out = st.open();
    out += &big(1600, "theme-studio");
    out += &format!(
        "{}\n\n",
        esc("Edit palettes and fonts with a live preview, then apply them system-wide.")
    );
    out += &big(1250, "Install");
    out += &format!(
        "Add the package to your flake, then run {} once.\n\n",
        code("theme-studio --init-config")
    );
    out += &big(1250, "Usage");
    for item in [
        "Pick a theme with o, edit it, and press a to apply.",
        "F2 downloads fonts; F3 holds the keymaps and history.",
        "Every save keeps the previous file, so nothing is lost.",
    ] {
        out += &format!("  •  {}\n", esc(item));
    }
    out += "\n";
    out += &format!(
        "<span background=\"{}\">{}</span>\n\n",
        st.c("bg2"),
        esc("  $ nix build .#theme-studio\n  $ ./result/bin/theme-studio --fonts  ")
    );
    out += &big(1250, "Keys");
    for row in [
        "┌──────────┬────────────────────┐",
        "│ key      │ does               │",
        "├──────────┼────────────────────┤",
        "│ s        │ save the theme     │",
        "│ <leader>u│ undo a config edit │",
        "└──────────┴────────────────────┘",
    ] {
        out += &format!("{}\n", esc(row));
    }
    out += &format!(
        "\n<span foreground=\"{}\">│ </span><i>{}</i>\n",
        st.c("grey1"),
        esc("Tip: fonts are previewed before they're installed.")
    );
    out + "</span>"
}

/// The default text for a font: code for monospaced fonts, icons for Nerd
/// Fonts, prose otherwise.
pub fn default_kind(category: &str, nerd: bool) -> Kind {
    if nerd {
        Kind::Icons
    } else if category == "monospace" {
        Kind::Code
    } else {
        Kind::Prose
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_are_valid_markup() {
        let p = Palette::blank();
        let st = Style {
            family: "Inter <Bold>",
            size: 12,
            palette: &p,
        };
        for (lang, l) in LANGS.iter().enumerate() {
            let m = code(&st, lang, 0);
            assert!(m.starts_with("<span font=\"Inter &lt;Bold&gt; 12\""));
            assert_eq!(
                m.matches("<span").count(),
                m.matches("</span>").count(),
                "{}",
                l.0
            );
            assert!(!m.contains("<<") && !m.contains("& "), "escaped");
        }
        assert!(code(&st, 0, 0).contains(">fn</span>"), "keywords coloured");
        for m in [icons(&st), readme(&st)] {
            assert_eq!(m.matches("<span").count(), m.matches("</span>").count());
        }
        let paras = prose();
        assert!(paras[1] == "Chapter 1" && paras[2].starts_with("I am by birth a Genevese"));
        let (page, next) = prose_page(&st, 0, 1500);
        assert!(page.contains("Genevese") && next > 2 && next < paras.len());
        let (_, last) = prose_page(&st, paras.len() - 1, 1500);
        assert_eq!(last, paras.len() - 1, "the last page stays");
        assert_eq!(default_kind("monospace", false), Kind::Code);
        assert_eq!(default_kind("serif", false), Kind::Prose);
        assert_eq!(default_kind("nerd font", true), Kind::Icons);
    }

    /// Pango accepts every text (skipped without ImageMagick).
    #[test]
    fn pango_renders_every_text() {
        if std::process::Command::new("magick")
            .arg("-version")
            .output()
            .is_err()
        {
            eprintln!("skipped: no ImageMagick");
            return;
        }
        let p = Palette::blank();
        let st = Style {
            family: "DejaVu Sans Mono",
            size: 11,
            palette: &p,
        };
        let dir = std::env::temp_dir().join(format!("theme-studio-samples-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut texts: Vec<(String, String)> = (0..LANGS.len())
            .map(|l| (LANGS[l].0.replace('/', "-"), code(&st, l, 0)))
            .collect();
        texts.push(("icons".into(), icons(&st)));
        texts.push(("readme".into(), readme(&st)));
        texts.push(("prose".into(), prose_page(&st, 0, 2000).0));
        for (name, markup) in texts {
            let f = dir.join(format!("{name}.markup"));
            std::fs::write(&f, &markup).unwrap();
            let out = std::process::Command::new("magick")
                .arg(format!("pango:@{}", f.display()))
                .arg(dir.join(format!("{name}.png")))
                .output()
                .unwrap();
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(
                out.status.success() && !err.contains("Pango"),
                "{name}: {err}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
