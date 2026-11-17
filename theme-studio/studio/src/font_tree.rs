//! Groups installed font families into a two-level tree for the font list:
//! design (JetBrains Mono, Noto, TeX Gyre) → variant (Nerd Font, Sans CJK)
//! → families. Groups with a single member are flattened into their parent.

use std::collections::BTreeMap;

use crate::fonts::Installed;

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Group {
        name: String,
        children: Vec<Node>,
    },
    /// Index into the installed fonts.
    Leaf(usize),
}

impl Node {
    /// Every font under this node.
    pub fn leaves(&self) -> Vec<usize> {
        match self {
            Node::Leaf(i) => vec![*i],
            Node::Group { children, .. } => children.iter().flat_map(Node::leaves).collect(),
        }
    }
}

/// Region codes on CJK font names: "Noto Sans CJK JP" → variant "Sans CJK".
const REGIONS: [&str; 7] = ["HK", "JP", "KR", "SC", "TC", "CN", "TW"];

/// "JetBrainsMono" → "JetBrains Mono", "GeistMono" → "Geist Mono": splits
/// off the last capitalised word (only used for Nerd Font names, whose base
/// names are written without spaces).
fn split_camel(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    match (1..chars.len())
        .rev()
        .find(|&i| chars[i].is_uppercase() && chars[i - 1].is_lowercase())
    {
        Some(i) => format!(
            "{} {}",
            chars[..i].iter().collect::<String>(),
            chars[i..].iter().collect::<String>()
        ),
        None => s.to_string(),
    }
}

/// (design tokens, variant tokens) for a family name, before designs are
/// merged across fonts.
fn classify(name: &str) -> (Vec<String>, Vec<String>, bool) {
    if let Some(pos) = name.find(" Nerd Font") {
        let mut base = &name[..pos];
        let nl = base.len() > 2 && base.ends_with("NL");
        if nl {
            base = &base[..base.len() - 2];
        }
        let variant = if nl { "NL Nerd Font" } else { "Nerd Font" };
        return (vec![split_camel(base)], vec![variant.to_string()], true);
    }
    let tokens: Vec<String> = name
        .split([' ', '-'])
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    let (first, rest) = tokens
        .split_first()
        .map(|(f, r)| (f.clone(), r.to_vec()))
        .unwrap_or_default();
    (vec![first], rest, false)
}

/// A design while grouping: its name, its fonts with their variant words,
/// and whether it is a Nerd Font design.
type Design = (String, Vec<(usize, Vec<String>)>, bool);

pub fn build(fonts: &[Installed]) -> Vec<Node> {
    // design → [(font, variant tokens)]
    let mut designs: BTreeMap<String, Design> = BTreeMap::new();
    for (i, f) in fonts.iter().enumerate() {
        let (design, variant, nerd) = classify(&f.family);
        let name = design.join(" ");
        designs
            .entry(name.to_lowercase())
            .or_insert_with(|| (name.clone(), Vec::new(), nerd))
            .1
            .push((i, variant));
    }

    let mut top: Vec<(String, Node)> = Vec::new();
    for (_, (mut name, mut members, nerd)) in designs {
        // Longer shared names: every "TeX Gyre X" shares "Gyre", so the
        // design is "TeX Gyre" and the variants are Adventor, Bonum, …
        while !nerd
            && members.len() > 1
            && members.iter().all(|(_, v)| v.len() > 1)
            && members.iter().all(|(_, v)| v[0] == members[0].1[0])
        {
            name = format!("{name} {}", members[0].1[0]);
            for (_, v) in members.iter_mut() {
                v.remove(0);
            }
        }
        // variant name → fonts (region codes dropped)
        let mut variants: BTreeMap<String, (String, Vec<usize>)> = BTreeMap::new();
        for (i, mut v) in members {
            if !nerd && v.last().is_some_and(|t| REGIONS.contains(&t.as_str())) {
                v.pop();
            }
            let vname = v.join(" ");
            variants
                .entry(vname.to_lowercase())
                .or_insert_with(|| (vname, Vec::new()))
                .1
                .push(i);
        }
        let mut children: Vec<(String, Node)> = Vec::new();
        for (_, (vname, leaves)) in variants {
            if leaves.len() > 1 && !vname.is_empty() {
                let mut kids: Vec<Node> = leaves.into_iter().map(Node::Leaf).collect();
                kids.sort_by_key(|n| leaf_name(n, fonts));
                children.push((
                    vname.to_lowercase(),
                    Node::Group {
                        name: vname,
                        children: kids,
                    },
                ));
            } else {
                for i in leaves {
                    children.push((fonts[i].family.to_lowercase(), Node::Leaf(i)));
                }
            }
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        let mut children: Vec<Node> = children.into_iter().map(|(_, n)| n).collect();
        // A design with a single variant group: its fonts sit directly under it.
        if children.len() == 1
            && let Node::Group {
                children: inner, ..
            } = &children[0]
        {
            children = inner.clone();
        }
        let node = if children.len() == 1 {
            children.remove(0)
        } else {
            Node::Group {
                name: name.clone(),
                children,
            }
        };
        top.push((name.to_lowercase(), node));
    }
    top.sort_by(|a, b| a.0.cmp(&b.0));
    top.into_iter().map(|(_, n)| n).collect()
}

fn leaf_name(n: &Node, fonts: &[Installed]) -> String {
    match n {
        Node::Leaf(i) => fonts[*i].family.to_lowercase(),
        Node::Group { name, .. } => name.to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(names: &[&str]) -> Vec<Installed> {
        names
            .iter()
            .map(|n| Installed {
                family: n.to_string(),
                mono: false,
                nerd: false,
                bitmap: false,
                special: false,
            })
            .collect()
    }

    fn shape(nodes: &[Node], fonts: &[Installed]) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(n: &Node, depth: usize, fonts: &[Installed], out: &mut Vec<String>) {
            match n {
                Node::Leaf(i) => out.push(format!("{}{}", "  ".repeat(depth), fonts[*i].family)),
                Node::Group { name, children } => {
                    out.push(format!("{}[{name}]", "  ".repeat(depth)));
                    for c in children {
                        walk(c, depth + 1, fonts, out);
                    }
                }
            }
        }
        for n in nodes {
            walk(n, 0, fonts, &mut out);
        }
        out
    }

    /// The fonts on this machine, grouped as agreed: design → variant → font.
    #[test]
    fn groups_real_font_names() {
        let fonts = installed(&[
            "DejaVu Sans",
            "DejaVu Sans Mono",
            "DejaVu Serif",
            "DejaVu Math TeX Gyre",
            "FreeMono",
            "FreeSans",
            "cursor.pcf",
            "GeistMono Nerd Font",
            "GeistMono Nerd Font Mono",
            "GeistMono Nerd Font Propo",
            "JetBrainsMono Nerd Font",
            "JetBrainsMono Nerd Font Mono",
            "JetBrainsMono Nerd Font Propo",
            "JetBrainsMonoNL Nerd Font",
            "JetBrainsMonoNL Nerd Font Mono",
            "JetBrainsMonoNL Nerd Font Propo",
            "Noto Color Emoji",
            "Noto Sans CJK HK",
            "Noto Sans CJK JP",
            "Noto Serif CJK HK",
            "Noto Serif CJK JP",
            "Noto Sans Mono CJK HK",
            "Noto Sans Mono CJK JP",
            "TeX Gyre Adventor",
            "TeX Gyre Heros",
            "TeX Gyre Heros Cn",
            "Unifont",
            "Unifont-JP",
            "Unifont Upper",
        ]);
        let tree = build(&fonts);
        let s = shape(&tree, &fonts);
        let expect = [
            "cursor.pcf",
            "[DejaVu]",
            "  DejaVu Math TeX Gyre",
            "  DejaVu Sans",
            "  DejaVu Sans Mono",
            "  DejaVu Serif",
            "FreeMono",
            "FreeSans",
            "[Geist Mono]",
            "  GeistMono Nerd Font",
            "  GeistMono Nerd Font Mono",
            "  GeistMono Nerd Font Propo",
            "[JetBrains Mono]",
            "  [Nerd Font]",
            "    JetBrainsMono Nerd Font",
            "    JetBrainsMono Nerd Font Mono",
            "    JetBrainsMono Nerd Font Propo",
            "  [NL Nerd Font]",
            "    JetBrainsMonoNL Nerd Font",
            "    JetBrainsMonoNL Nerd Font Mono",
            "    JetBrainsMonoNL Nerd Font Propo",
            "[Noto]",
            "  Noto Color Emoji",
            "  [Sans CJK]",
            "    Noto Sans CJK HK",
            "    Noto Sans CJK JP",
            "  [Sans Mono CJK]",
            "    Noto Sans Mono CJK HK",
            "    Noto Sans Mono CJK JP",
            "  [Serif CJK]",
            "    Noto Serif CJK HK",
            "    Noto Serif CJK JP",
            "[TeX Gyre]",
            "  TeX Gyre Adventor",
            "  TeX Gyre Heros",
            "  TeX Gyre Heros Cn",
            "[Unifont]",
            "  Unifont",
            "  Unifont Upper",
            "  Unifont-JP",
        ];
        assert_eq!(s, expect, "\n{}", s.join("\n"));
        let all: usize = tree.iter().map(|n| n.leaves().len()).sum();
        assert_eq!(all, fonts.len(), "every font appears once");
    }
}
