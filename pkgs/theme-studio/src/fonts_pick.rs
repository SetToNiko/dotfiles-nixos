//! Picking fonts for the roles: the list opens with the role's filter on
//! (a setting per role), the user's own filters come back afterwards, and a
//! whole family can fill all three roles at once. A pick sets `picked`, so
//! the app saves and applies it straight away.

use crate::fonts::{FontFocus, FontsTab, Installed, NerdKind, ROLES, Row, Spacing};

/// The choices for a role's filter (settings.json `fonts.<role>-filter`).
pub const ROLE_FILTERS: [&str; 6] = [
    "any",
    "mono",
    "propo",
    "nerd-font",
    "nerd-font-mono",
    "nerd-font-propo",
];

/// The default filter per role: mono monospaced, ui proportional, bar with
/// icons (Nerd Font).
pub const ROLE_FILTER_DEFAULTS: [&str; 3] = ["mono", "propo", "nerd-font"];

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

impl FontsTab {
    /// Enter on a role: the list, with that role's filter on.
    pub fn start_pick(&mut self) {
        if self.picking.is_none() {
            self.saved_filters = Some(self.filters);
        }
        let mut f = self.saved_filters.unwrap_or(self.filters);
        let (spacing, nerd) = match self.role_filters[self.slot].as_str() {
            "mono" => (Spacing::Mono, NerdKind::Any),
            "propo" => (Spacing::Propo, NerdKind::Any),
            "nerd-font" => (Spacing::Any, NerdKind::Nerd),
            "nerd-font-mono" => (Spacing::Any, NerdKind::NerdMono),
            "nerd-font-propo" => (Spacing::Any, NerdKind::NerdPropo),
            _ => (Spacing::Any, NerdKind::Any),
        };
        f.spacing = spacing;
        f.nerd = nerd;
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
    /// highlighted font). Returns what went where, or why not.
    pub fn fill_from_family(&mut self) -> Result<String, String> {
        let rows = self.rows();
        let group = match rows.get(self.list_sel) {
            Some(Row::Group { path, .. }) => path.split('/').next().unwrap_or("").to_string(),
            // a font: its group, or (not in one) its own name
            Some(Row::Font { index, .. }) => match self.path_to(*index).first() {
                Some(p) => p.clone(),
                None => self.installed[*index].family.clone(),
            },
            None => return Err("Nothing highlighted".into()),
        };
        self.fill_roles(&group, &[0, 1, 2])
    }

    /// Fills `roles` from a family (a font tree group or a family name),
    /// each with its best face; a role it has no face for keeps its font.
    /// Returns what went where, or why not.
    pub fn fill_roles(&mut self, group: &str, roles: &[usize]) -> Result<String, String> {
        let group = group.to_string();
        // the group, and fonts named like it (the font tree can put a
        // family's Nerd Font variants in a group of their own)
        let norm = |s: &str| {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        };
        let key = norm(&group);
        let members: Vec<&Installed> = (0..self.installed.len())
            .filter(|&i| {
                self.path_to(i).first() == Some(&group)
                    || norm(&self.installed[i].family).starts_with(&key)
            })
            .map(|i| &self.installed[i])
            .filter(|f| !f.bitmap)
            .collect();
        if members.is_empty() {
            return Err(format!("No fonts in {group}"));
        }
        // the shortest name of a kind is the family's plain face
        let best = |kinds: &[&str]| -> Option<String> {
            kinds.iter().find_map(|k| {
                members
                    .iter()
                    .filter(|f| kind(f) == *k)
                    .map(|f| f.family.clone())
                    .min_by_key(|n| n.len())
            })
        };
        let picks = [
            best(&["nf-mono", "mono", "nf"]),
            best(&["nf-propo", "propo", "nf", "mono"]),
            best(&["nf", "nf-propo", "propo", "nf-mono", "mono"]),
        ];
        let mut done = Vec::new();
        let mut kept = Vec::new();
        for (role, pick) in picks.iter().enumerate().filter(|(r, _)| roles.contains(r)) {
            match pick {
                Some(family) => {
                    self.preset.fonts[role].family = family.clone();
                    done.push(format!("{}: {family}", ROLES[role]));
                }
                None => kept.push(ROLES[role]),
            }
        }
        self.end_pick();
        self.focus = FontFocus::Slots;
        self.picked = true;
        let mut msg = format!("From {group}: {}", done.join(", "));
        if !kept.is_empty() {
            msg += &format!(" ({} kept: no face for it)", kept.join(", "));
        }
        Ok(msg)
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
