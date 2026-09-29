use super::{
    bounded_settings_text_edit_width, bounded_singleline_text_edit_with_hint, settings_control_row,
    settings_switch,
};
use crate::ui_icons::{IconKind, icon_button};
use eframe::egui;
use kuroya_core::EditorSettings;

const NOTIFICATIONS_TITLE: &str = "Notifications";
const NOTIFICATIONS_DESCRIPTION: &str = "Suppress these notification popups. The underlying actions still run — only the popups are hidden.";
const CUSTOM_CATEGORIES_TITLE: &str = "Custom categories";
const CUSTOM_CATEGORIES_DESCRIPTION: &str = "Mute a notification category id that has no row above, for example from a future or add-on feature.";
const CUSTOM_CATEGORY_INPUT_HINT: &str = "category-id";
const CUSTOM_CATEGORY_INPUT_ID: &str = "settings_muted_notifications_custom_category_input";

const MUTED_NOTIFICATION_CATEGORIES: [(&str, &str); 9] = [
    ("lsp", "LSP notices"),
    ("lsp-install", "LSP installs"),
    ("update", "Updates"),
    ("background-image", "Background image"),
    ("indexing", "Indexing"),
    ("git", "Git changes"),
    ("slow-frames", "Slow frames"),
    ("plugins", "Plugins"),
    ("general", "General"),
];

pub(super) fn render_notification_mute_settings(ui: &mut egui::Ui, draft: &mut EditorSettings) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(NOTIFICATIONS_TITLE).strong())
        .on_hover_text(NOTIFICATIONS_DESCRIPTION);

    for (category_id, label) in MUTED_NOTIFICATION_CATEGORIES {
        render_known_category_row(ui, &mut draft.muted_notifications, category_id, label);
    }

    render_custom_muted_categories(ui, &mut draft.muted_notifications);
}

fn render_known_category_row(
    ui: &mut egui::Ui,
    muted: &mut Vec<String>,
    category_id: &str,
    label: &str,
) {
    let mut is_muted = category_is_muted(muted, category_id);
    settings_control_row(
        ui,
        label,
        &format!("Notification category id: {category_id}"),
        |ui| {
            settings_switch(ui, &mut is_muted, &format!("Mute {label}"))
                .on_hover_text(NOTIFICATIONS_DESCRIPTION);
        },
    );
    set_category_muted(muted, category_id, is_muted);
}

fn render_custom_muted_categories(ui: &mut egui::Ui, muted: &mut Vec<String>) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(CUSTOM_CATEGORIES_TITLE).strong());
        ui.label(
            egui::RichText::new(CUSTOM_CATEGORIES_DESCRIPTION)
                .small()
                .color(ui.visuals().weak_text_color()),
        )
        .on_hover_text(NOTIFICATIONS_DESCRIPTION);
    });

    let mut removed_category = None;
    for (index, category) in custom_muted_categories(muted).into_iter().enumerate() {
        ui.push_id(("muted_notification_custom_category", index), |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&category).monospace())
                    .on_hover_text(NOTIFICATIONS_DESCRIPTION);
                if icon_button(ui, IconKind::Trash, "Remove custom muted category").clicked() {
                    removed_category = Some(category);
                }
            });
        });
    }
    if let Some(category) = removed_category {
        set_category_muted(muted, &category, false);
    }

    render_custom_category_input(ui, muted);
}

fn render_custom_category_input(ui: &mut egui::Ui, muted: &mut Vec<String>) {
    let input_id = egui::Id::new(CUSTOM_CATEGORY_INPUT_ID);
    let mut pending_category =
        ui.data_mut(|data| data.get_temp::<String>(input_id).unwrap_or_default());
    let mut add_requested = false;

    ui.horizontal(|ui| {
        let input_width = bounded_settings_text_edit_width(ui.available_width(), 280.0);
        if bounded_singleline_text_edit_with_hint(
            ui,
            &mut pending_category,
            input_width,
            Some(CUSTOM_CATEGORY_INPUT_HINT),
        )
        .on_hover_text(NOTIFICATIONS_DESCRIPTION)
        .lost_focus()
            && ui.input(|input| input.key_pressed(egui::Key::Enter))
        {
            add_requested = true;
        }
        if icon_button(ui, IconKind::Plus, "Add custom muted category").clicked() {
            add_requested = true;
        }
    });

    if add_requested && add_custom_muted_category(muted, &pending_category) {
        pending_category.clear();
    }

    ui.data_mut(|data| data.insert_temp(input_id, pending_category));
}

fn category_is_muted(muted: &[String], category_id: &str) -> bool {
    muted.iter().any(|muted| muted == category_id)
}

fn set_category_muted(muted: &mut Vec<String>, category_id: &str, is_muted: bool) {
    if is_muted {
        if !category_is_muted(muted, category_id) {
            muted.push(category_id.to_owned());
        }
    } else {
        muted.retain(|muted| muted != category_id);
    }
}

fn is_known_notification_category(category_id: &str) -> bool {
    MUTED_NOTIFICATION_CATEGORIES
        .iter()
        .any(|(known_id, _)| *known_id == category_id)
}

fn custom_muted_categories(muted: &[String]) -> Vec<String> {
    let mut custom = Vec::new();
    for category in muted {
        if !is_known_notification_category(category) && !custom.contains(category) {
            custom.push(category.clone());
        }
    }
    custom
}

fn add_custom_muted_category(muted: &mut Vec<String>, raw_category: &str) -> bool {
    let category = raw_category.trim();
    if category.is_empty()
        || is_known_notification_category(category)
        || category_is_muted(muted, category)
    {
        return false;
    }
    muted.push(category.to_owned());
    true
}

#[cfg(test)]
mod tests {
    use super::{
        add_custom_muted_category, category_is_muted, custom_muted_categories,
        render_notification_mute_settings, set_category_muted,
    };
    use eframe::egui;
    use kuroya_core::EditorSettings;

    #[test]
    fn editor_settings_default_has_no_muted_notifications() {
        assert!(EditorSettings::default().muted_notifications.is_empty());
    }

    #[test]
    fn muted_category_toggles_add_and_remove_known_categories_without_duplicates() {
        let mut muted = Vec::new();

        set_category_muted(&mut muted, "lsp", true);
        assert!(category_is_muted(&muted, "lsp"));
        set_category_muted(&mut muted, "lsp", true);
        assert_eq!(muted, ["lsp".to_owned()]);

        set_category_muted(&mut muted, "update", true);
        assert_eq!(muted, ["lsp".to_owned(), "update".to_owned()]);

        set_category_muted(&mut muted, "lsp", false);
        assert_eq!(muted, ["update".to_owned()]);
        set_category_muted(&mut muted, "lsp", false);
        assert_eq!(muted, ["update".to_owned()]);
    }

    #[test]
    fn custom_muted_category_add_and_remove_updates_muted_list() {
        let mut muted = vec!["git".to_owned()];

        assert!(add_custom_muted_category(&mut muted, " team-standup "));
        assert_eq!(muted, ["git".to_owned(), "team-standup".to_owned()]);

        assert!(!add_custom_muted_category(&mut muted, "   "));
        assert!(!add_custom_muted_category(&mut muted, " team-standup "));
        assert!(!add_custom_muted_category(&mut muted, "lsp"));
        assert_eq!(muted, ["git".to_owned(), "team-standup".to_owned()]);

        set_category_muted(&mut muted, "team-standup", false);
        assert_eq!(muted, ["git".to_owned()]);
        set_category_muted(&mut muted, "missing-category", false);
        assert_eq!(muted, ["git".to_owned()]);
    }

    #[test]
    fn custom_muted_categories_lists_unknown_ids_in_order_without_duplicates() {
        let muted = vec![
            "lsp".to_owned(),
            "team-standup".to_owned(),
            "general".to_owned(),
            "team-standup".to_owned(),
            "release-bot".to_owned(),
        ];

        assert_eq!(
            custom_muted_categories(&muted),
            ["team-standup".to_owned(), "release-bot".to_owned()]
        );
    }

    #[test]
    fn notifications_section_render_preserves_mute_state_and_custom_categories() {
        let ctx = egui::Context::default();
        let mut draft = EditorSettings {
            muted_notifications: vec!["lsp".to_owned(), "team-standup".to_owned()],
            ..EditorSettings::default()
        };

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_notification_mute_settings(ui, &mut draft);
            });
        });

        assert_eq!(
            draft.muted_notifications,
            ["lsp".to_owned(), "team-standup".to_owned()]
        );
    }
}
