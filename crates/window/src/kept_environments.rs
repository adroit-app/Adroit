//! The environments a window shows beside its own, kept from one run of the
//! window to the next: one environment id a line, under the window's number.

use adroit_connect::environments::{Catalog, Environment};
use adroit_model::EnvironmentId;

/// The environments `text` keeps, in the order it keeps them.
pub(crate) fn read(text: &str) -> Vec<EnvironmentId> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(EnvironmentId::new)
        .collect()
}

/// `environments` as the window keeps them.
pub(crate) fn text<'a>(environments: impl IntoIterator<Item = &'a EnvironmentId>) -> String {
    environments
        .into_iter()
        .map(|environment| format!("{environment}\n"))
        .collect()
}

/// The environments of `kept` the window opens again, as `catalog` holds
/// them: one the catalog no longer holds, or holds archived, is left out,
/// and so is `own`, which the window opens in any case.
pub(crate) fn reopened(
    kept: &[EnvironmentId],
    catalog: &Catalog,
    own: &EnvironmentId,
) -> Vec<Environment> {
    kept.iter()
        .filter(|id| *id != own)
        .filter_map(|id| {
            catalog.environments.iter().find(|environment| {
                EnvironmentId::new(&environment.id) == *id && !environment.archived
            })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use adroit_connect::Target;

    use super::*;

    fn environment(id: &str, archived: bool) -> Environment {
        Environment {
            id: id.into(),
            name: id.to_uppercase(),
            target: Target::Local,
            scope: Some(id.into()),
            archived,
            legacy_client: None,
            work_instance: None,
        }
    }

    #[test]
    fn the_environments_kept_read_back_as_they_were_kept() {
        let kept = [EnvironmentId::new("b"), EnvironmentId::new("a")];
        assert_eq!(read(&text(&kept)), kept);
    }

    #[test]
    fn nothing_kept_and_blank_lines_read_as_no_environment() {
        assert_eq!(read(""), Vec::<EnvironmentId>::new());
        assert_eq!(read("\n a \n\n"), [EnvironmentId::new("a")]);
    }

    #[test]
    fn an_environment_removed_or_archived_is_not_opened_again_nor_the_window_s_own() {
        let catalog = Catalog {
            environments: vec![
                environment("own", false),
                environment("kept", false),
                environment("archived", true),
            ],
            ..Catalog::default()
        };
        let kept = ["own", "removed", "archived", "kept"].map(EnvironmentId::new);
        let reopened = reopened(&kept, &catalog, &EnvironmentId::new("own"));
        assert_eq!(reopened, [environment("kept", false)]);
    }
}
