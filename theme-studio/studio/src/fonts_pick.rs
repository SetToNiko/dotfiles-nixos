//! Picking fonts for the roles. Each role needs a kind of font: Monospace a
//! Nerd Font Mono, Proportional a proportional font, Icons a Nerd Font
//! Propo. While picking, the font list shows only fonts that fit (the
//! spacing and Nerd Font chips are locked), and the user's own filters come
//! back afterwards. A whole family can fill the roles at once, each with
//! its fitting face. A pick sets `picked`, so the app saves and applies it
//! straight away.

use crate::fonts::{FontFocus, FontsTab, Installed, ROLE_NAMES, ROLE_NEEDS, Row};

fn kind(f: &Installed) -> &'static str {
    if f.family.ends_with("Nerd Font Mono") {
        "nf-mono"
    } else if f.family.ends_with("Nerd Font Propo") {
        "nf-propo"
    } else if f.nerd {
        "nf"
    } else if f.mono {
        "mono"
    } else {
        "propo"
    }
}

/// Whether a font meets a role's requirement.
pub fn fits(role: usize, f: &Installed) -> bool {
    match role {
        0 => kind(f) == "nf-mono",
        1 => !f.mono,
        _ => kind(f) == "nf-propo",
    }
}

/// Letters and digits only, lower case: how family names are compared.
pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// A family's name without a Nerd Font suffix ("GeistMono Nerd Font Mono"
/// → "GeistMono").
pub fn base_family(family: &str) -> &str {
    ["Nerd Font Mono", "Nerd Font Propo", "Nerd Font"]
        .iter()
        .find_map(|s| family.strip_suffix(s))
        .map(str::trim_end)
        .unwrap_or(family)
}

/// The faces roles need, for messages: "Nerd Font Mono or proportional face".
pub fn faces(roles: &[usize]) -> String {
    let needs: Vec<&str> = roles
        .iter()
        .map(|&r| ROLE_NEEDS[r].trim_start_matches("a "))
        .collect();
    let list = match needs.split_last() {
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    };
    format!("{list} face")
}

/// What filling roles from a family did.
#[derive(Debug, Default, PartialEq)]
pub struct Fill {
    /// The family (a font tree group or a family name).
    pub group: String,
    /// What went where, and what was kept (for the status line).
    pub msg: String,
    /// Some role got a new font (the app saves and applies it).
    pub changed: bool,
    /// Roles the family has no fitting face for.
    pub missing: Vec<usize>,
}

impl FontsTab {
    /// Enter on a role: the list, showing only the fonts that fit it.
    pub fn start_pick(&mut self) {
        if self.picking.is_none() {
            self.saved_filters = Some(self.filters);
        }
        let mut f = self.saved_filters.unwrap_or(self.filters);
        f.spacing = Default::default();
        f.nerd = Default::default();
        f.need = Some(self.slot);
        self.filters = f;
        self.picking = Some(self.slot);
        self.open_list();
    }

    /// Leaving the list (a pick or Back): the user's filters come back.
    pub fn end_pick(&mut self) {
        if self.picking.take().is_some()
            && let Some(f) = self.saved_filters.take()
        {
            self.filters = f;
            self.clamp_sel();
        }
    }

    /// `F`: every role from the highlighted family (or the family of the
    /// highlighted font).
    pub fn fill_from_family(&mut self) -> Result<Fill, String> {
        let rows = self.rows();
        let group = match rows.get(self.list_sel) {
            Some(Row::Group { path, .. }) => path.split('/').next().unwrap_or("").to_string(),
            // a font: its group, or (not in one) its own name
            Some(Row::Font { index, .. }) => self.family_of(*index),
            None => return Err("Nothing highlighted".into()),
        };
        Ok(self.fill_roles(&group, &[0, 1, 2]))
    }

    /// The family an installed font belongs to: its group in the font tree,
    /// or its name without a Nerd Font suffix.
    fn family_of(&self, index: usize) -> String {
        match self.path_to(index).first() {
            Some(p) => p.clone(),
            None => base_family(&self.installed[index].family).to_string(),
        }
    }

    /// `r` on a role: the same family's face that fits it, in place of a
    /// font that doesn't.
    pub fn fix_role(&mut self) -> Result<Fill, String> {
        let role = self.slot;
        let family = self.preset.fonts[role].family.clone();
        let index = self
            .installed
            .iter()
            .position(|f| f.family.eq_ignore_ascii_case(&family));
        if let Some(i) = index
            && fits(role, &self.installed[i])
        {
            return Err(format!("{family} already fits {}", ROLE_NAMES[role]));
        }
        let group = match index {
            Some(i) => self.family_of(i),
            None => base_family(&family).to_string(),
        };
        Ok(self.fill_roles(&group, &[role]))
    }

    /// Fills `roles` from a family (a font tree group or a family name),
    /// each with its fitting face (the shortest name of those, the plain
    /// one); a role it has none for keeps its font.
    pub fn fill_roles(&mut self, group: &str, roles: &[usize]) -> Fill {
        // the group, and fonts named like it (the font tree can put a
        // family's Nerd Font variants in a group of their own)
        let key = norm(base_family(group));
        let members: Vec<&Installed> = (0..self.installed.len())
            .filter(|&i| {
                self.path_to(i).first().map(String::as_str) == Some(group)
                    || (!key.is_empty() && norm(&self.installed[i].family).starts_with(&key))
            })
            .map(|i| &self.installed[i])
            .filter(|f| !f.bitmap)
            .collect();
        let mut fill = Fill {
            group: group.to_string(),
            ..Default::default()
        };
        let mut done = Vec::new();
        let mut kept = Vec::new();
        for &role in roles {
            let pick = members
                .iter()
                .filter(|f| fits(role, f))
                .map(|f| f.family.clone())
                .min_by_key(|n| n.len());
            match pick {
                Some(family) => {
                    if self.preset.fonts[role].family != family {
                        fill.changed = true;
                    }
                    self.preset.fonts[role].family = family.clone();
                    done.push(format!("{}: {family}", ROLE_NAMES[role]));
                }
                None => {
                    kept.push(ROLE_NAMES[role]);
                    fill.missing.push(role);
                }
            }
        }
        self.end_pick();
        self.focus = FontFocus::Slots;
        self.picked = fill.changed;
        let needs = faces(&fill.missing);
        fill.msg = match (done.is_empty(), kept.is_empty()) {
            (_, true) => format!("From {group}: {}", done.join(", ")),
            (true, false) => format!("{group} has no {needs} ({} kept)", kept.join(", ")),
            (false, false) => format!(
                "From {group}: {} ({} kept: no {needs})",
                done.join(", "),
                kept.join(", ")
            ),
        };
        fill
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::Installed;

    fn font(family: &str, mono: bool, nerd: bool) -> Installed {
        Installed {
            family: family.into(),
            mono,
            nerd,
            bitmap: false,
            special: false,
        }
    }

    #[test]
    fn kinds() {
        assert_eq!(
            kind(&font("JetBrainsMono Nerd Font Mono", true, true)),
            "nf-mono"
        );
        assert_eq!(
            kind(&font("JetBrainsMono Nerd Font Propo", false, true)),
            "nf-propo"
        );
        assert_eq!(kind(&font("JetBrainsMono Nerd Font", true, true)), "nf");
        assert_eq!(kind(&font("JetBrains Mono", true, false)), "mono");
        assert_eq!(kind(&font("Inter", false, false)), "propo");
    }
}
