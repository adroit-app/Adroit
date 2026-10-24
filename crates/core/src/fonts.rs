//! The fonts the console can be drawn in: the monospace families installed
//! on this machine and adroit's own.

/// The family adroit's own font is named as, by the window and by the
/// system when the user has it installed as well.
pub const FAMILY: &str = "JetBrainsMono Nerd Font Mono";

/// One entry the Settings window's Font row offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontChoice {
    /// What `font_family` is set to when it is picked.
    pub value: String,
    pub label: String,
}

/// Whether every one of `widths` is there, and they are one width wider
/// than nothing.
pub fn equal_widths(mut widths: impl Iterator<Item = Option<u16>>) -> bool {
    let Some(Some(first)) = widths.next() else {
        return false;
    };
    first > 0 && widths.all(|w| w == Some(first))
}

/// The families of the `(family, monospaced)` faces drawn in equal cells,
/// sorted and once each, leaving out those the system hides from users.
pub fn monospace_families<'a>(faces: impl Iterator<Item = (&'a str, bool)>) -> Vec<String> {
    let mut families: Vec<String> = faces
        .filter(|(family, mono)| *mono && !family.starts_with('.') && !family.trim().is_empty())
        .map(|(family, _)| family.to_string())
        .collect();
    families.sort_by_key(|f| (f.to_lowercase(), f.clone()));
    families.dedup();
    families
}

/// The choices for a saved `font_family` of `saved`, given the families
/// `installed`, or `None` while they are still being read: adroit's own
/// font, which `font_family` leaves unnamed, a saved family that is not
/// installed, then the installed ones. A saved family not installed stays a
/// choice, so it is not lost until another is picked.
pub fn choices(installed: Option<&[String]>, saved: &str) -> Vec<FontChoice> {
    let mut out = vec![FontChoice {
        value: String::new(),
        label: format!("{FAMILY} (built in)"),
    }];
    let known = saved.is_empty() || installed.is_some_and(|i| i.iter().any(|f| f == saved));
    if !known {
        out.push(FontChoice {
            value: saved.to_string(),
            label: match installed {
                Some(_) => format!("{saved} (not installed)"),
                None => saved.to_string(),
            },
        });
    }
    out.extend(
        installed
            .unwrap_or_default()
            .iter()
            .filter(|f| *f != FAMILY)
            .map(|f| FontChoice {
                value: f.clone(),
                label: f.clone(),
            }),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(choices: &[FontChoice]) -> Vec<&str> {
        choices.iter().map(|c| c.value.as_str()).collect()
    }

    fn installed_here() -> Vec<String> {
        vec!["Menlo".into(), FAMILY.into(), "Monaco".into()]
    }

    #[test]
    fn only_monospace_families_are_offered_sorted_once_each_without_hidden_ones() {
        let families = monospace_families(
            [
                ("Monaco", true),
                ("Helvetica", false),
                ("menlo", true),
                ("Menlo", true),
                ("Menlo", true),
                (".SF NS Mono", true),
                ("", true),
            ]
            .into_iter(),
        );
        assert_eq!(families, ["Menlo", "menlo", "Monaco"]);
    }

    #[test]
    fn letters_and_digits_all_as_wide_make_equal_cells() {
        assert!(equal_widths([Some(600), Some(600), Some(600)].into_iter()));
    }

    #[test]
    fn letters_of_differing_widths_or_missing_or_empty_do_not() {
        assert!(!equal_widths([Some(600), Some(300), Some(600)].into_iter()));
        assert!(!equal_widths([Some(600), None, Some(600)].into_iter()));
        assert!(!equal_widths([Some(0), Some(0)].into_iter()));
        assert!(!equal_widths(std::iter::empty()));
    }

    #[test]
    fn the_built_in_font_comes_first_as_the_font_left_unnamed() {
        let c = choices(Some(&installed_here()), "");
        assert_eq!(values(&c), ["", "Menlo", "Monaco"]);
        assert_eq!(c[0].label, format!("{FAMILY} (built in)"));
    }

    #[test]
    fn the_built_in_font_is_offered_before_the_installed_ones_are_read() {
        let c = choices(None, "");
        assert_eq!(values(&c), [""]);
    }

    #[test]
    fn an_installed_saved_family_is_offered_once() {
        let c = choices(Some(&installed_here()), "Menlo");
        assert_eq!(values(&c), ["", "Menlo", "Monaco"]);
    }

    #[test]
    fn a_saved_family_not_installed_is_kept_and_marked_so() {
        let c = choices(Some(&installed_here()), "Iosevka");
        assert_eq!(values(&c), ["", "Iosevka", "Menlo", "Monaco"]);
        assert_eq!(c[1].label, "Iosevka (not installed)");
    }

    #[test]
    fn a_saved_family_is_offered_unmarked_while_the_installed_ones_are_read() {
        let c = choices(None, "Iosevka");
        assert_eq!(values(&c), ["", "Iosevka"]);
        assert_eq!(c[1].label, "Iosevka");
    }
}
