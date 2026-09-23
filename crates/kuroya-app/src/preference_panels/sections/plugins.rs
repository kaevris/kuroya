use super::{
    SETTINGS_TARGET_PLUGINS, SettingsHighlightState, settings_control_row, settings_switch,
    settings_target_block, settings_toggle_row,
};
use eframe::egui;
use kuroya_core::{EditorSettings, PluginDescriptor};

const PLUGINS_TOGGLE_DESCRIPTION: &str =
    "Run plugins discovered under .kuroya/plugins. Untrusted workspaces always block plugins.";
const PLUGIN_ROW_DESCRIPTION: &str =
    "Toggle whether this discovered workspace plugin is allowed to run.";

pub(super) fn render_plugins_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    discovered_plugins: &[PluginDescriptor],
    highlight: &mut SettingsHighlightState<'_>,
) {
    settings_target_block(ui, highlight, SETTINGS_TARGET_PLUGINS, |ui| {
        render_plugins_settings_content(ui, draft, discovered_plugins);
    });
}

fn render_plugins_settings_content(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    discovered_plugins: &[PluginDescriptor],
) {
    settings_toggle_row(
        ui,
        "Enable workspace plugins",
        PLUGINS_TOGGLE_DESCRIPTION,
        &mut draft.plugins.enabled,
    );

    ui.add_space(8.0);
    ui.label(egui::RichText::new("Discovered plugins").strong());
    let plugin_ids = sorted_plugin_ids(discovered_plugins);
    if plugin_ids.is_empty() {
        ui.label(
            egui::RichText::new("No workspace plugins discovered.")
                .color(ui.visuals().weak_text_color()),
        );
        return;
    }

    for id in &plugin_ids {
        render_plugin_id_row(ui, &mut draft.plugins.disabled_ids, id);
    }
}

fn render_plugin_id_row(ui: &mut egui::Ui, disabled_ids: &mut Vec<String>, id: &str) {
    let mut enabled = !plugin_is_disabled(disabled_ids, id);
    settings_control_row(ui, id, PLUGIN_ROW_DESCRIPTION, |ui| {
        settings_switch(ui, &mut enabled, id);
    });
    set_plugin_disabled(disabled_ids, id, !enabled);
}

fn sorted_plugin_ids(plugins: &[PluginDescriptor]) -> Vec<&str> {
    let mut ids: Vec<&str> = plugins
        .iter()
        .map(|descriptor| descriptor.manifest.id.as_str())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn plugin_is_disabled(disabled_ids: &[String], id: &str) -> bool {
    disabled_ids.iter().any(|disabled| disabled == id)
}

fn set_plugin_disabled(disabled_ids: &mut Vec<String>, id: &str, disabled: bool) {
    if disabled {
        if !plugin_is_disabled(disabled_ids, id) {
            disabled_ids.push(id.to_owned());
        }
    } else {
        disabled_ids.retain(|disabled| disabled != id);
    }
}

#[cfg(test)]
mod tests {
    use super::{plugin_is_disabled, render_plugins_settings_with_highlight, set_plugin_disabled};
    use crate::preference_panels::sections::SettingsHighlightState;
    use eframe::egui;
    use kuroya_core::{
        EditorSettings, PluginCapabilities, PluginContributions, PluginDescriptor, PluginManifest,
    };

    #[test]
    fn plugin_membership_helpers_add_remove_and_dedup_ids() {
        let mut disabled_ids = Vec::new();

        assert!(!plugin_is_disabled(&disabled_ids, "alpha.plugin"));
        set_plugin_disabled(&mut disabled_ids, "alpha.plugin", true);
        assert_eq!(disabled_ids, ["alpha.plugin".to_owned()]);
        assert!(plugin_is_disabled(&disabled_ids, "alpha.plugin"));

        set_plugin_disabled(&mut disabled_ids, "alpha.plugin", true);
        assert_eq!(disabled_ids, ["alpha.plugin".to_owned()]);

        set_plugin_disabled(&mut disabled_ids, "beta.plugin", false);
        assert_eq!(disabled_ids, ["alpha.plugin".to_owned()]);

        set_plugin_disabled(&mut disabled_ids, "alpha.plugin", false);
        assert!(disabled_ids.is_empty());

        set_plugin_disabled(&mut disabled_ids, "missing.plugin", false);
        assert!(disabled_ids.is_empty());
    }

    #[test]
    fn plugins_section_render_keeps_discovered_rows_and_existing_memberships() {
        let ctx = egui::Context::default();
        let mut draft = EditorSettings {
            plugins: kuroya_core::settings::PluginSettings {
                enabled: true,
                disabled_ids: vec!["gamma.plugin".to_owned()],
            },
            ..EditorSettings::default()
        };
        let discovered = [
            test_plugin_descriptor("beta.plugin"),
            test_plugin_descriptor("alpha.plugin"),
        ];

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut highlight = SettingsHighlightState::disabled();
                render_plugins_settings_with_highlight(ui, &mut draft, &discovered, &mut highlight);
            });
        });

        assert_eq!(draft.plugins.disabled_ids, ["gamma.plugin".to_owned()]);
    }

    fn test_plugin_descriptor(id: &str) -> PluginDescriptor {
        PluginDescriptor {
            root: std::path::PathBuf::from(format!(".kuroya/plugins/{id}")),
            manifest: PluginManifest {
                api_version: "1".to_owned(),
                id: id.to_owned(),
                name: id.to_owned(),
                version: "0.1.0".to_owned(),
                entry: None,
                activation_events: Vec::new(),
                capabilities: PluginCapabilities::default(),
                contributes: PluginContributions::default(),
            },
        }
    }
}
