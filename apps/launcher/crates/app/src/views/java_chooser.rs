//! One searchable runtime catalog for launcher defaults and instance overrides.
use super::LauncherRoot;
use crate::{
    input::{FieldState, NativeField, NativeFieldEvent},
    ui::{btn, card, list_item, ButtonVariant},
};
use gpui::{div, prelude::*, px, Context, Div};
use industrialis_backend::java::JavaInfo;
use industrialis_core::natural::compare_natural;

#[derive(Clone, Copy, Default, PartialEq)]
enum Column {
    #[default]
    Version,
    Architecture,
    Path,
}
#[derive(Default)]
pub(super) struct JavaChooser {
    search: String,
    column: Column,
    descending: bool,
}
impl JavaChooser {
    fn visible(&self, items: &[JavaInfo]) -> Vec<JavaInfo> {
        let query = self.search.to_lowercase();
        let mut found: Vec<_> = items
            .iter()
            .filter(|j| {
                format!("{} {} {} {}", j.path, j.version, j.architecture, j.vendor)
                    .to_lowercase()
                    .contains(&query)
            })
            .cloned()
            .collect();
        found.sort_by(|a, b| {
            let order = match self.column {
                Column::Version => compare_natural(&a.version, &b.version),
                Column::Architecture => a.architecture.cmp(&b.architecture),
                Column::Path => a.path.cmp(&b.path),
            }
            .then_with(|| a.path.cmp(&b.path));
            if self.descending {
                order.reverse()
            } else {
                order
            }
        });
        found
    }
}
impl LauncherRoot {
    pub(super) fn java_chooser(
        &mut self,
        target: Option<String>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        if self.preferences_ui.installations.is_none() && !self.preferences_ui.scanning {
            self.scan_launcher_java(cx);
        }
        let field_key = "java-chooser-search".to_string();
        if !self.native_fields.contains_key(&field_key) {
            let field = cx.new(|cx| {
                NativeField::new(
                    &self.java_ui.search,
                    self.theme.clone(),
                    "Search Java runtimes",
                    false,
                    cx,
                )
            });
            cx.subscribe(&field, |v, _, event, cx| {
                if let NativeFieldEvent::Change(text) = event {
                    v.java_ui.search = text.clone();
                    cx.notify();
                }
            })
            .detach();
            self.native_fields.insert(field_key.clone(), field);
        }
        let field = self.native_fields[&field_key].clone();
        field.update(cx, |state, _| {
            state.value = self.java_ui.search.clone();
            state.theme = self.theme.clone();
        });
        let items = self.java_ui.visible(
            self.preferences_ui
                .installations
                .as_deref()
                .unwrap_or_default(),
        );
        let t = self.theme.clone();
        let mut panel = card(&t)
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Installed runtimes"),
            )
            .child(field)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .children(
                        [
                            (Column::Version, "Version"),
                            (Column::Architecture, "Architecture"),
                            (Column::Path, "Path"),
                        ]
                        .into_iter()
                        .map(|(column, label)| {
                            btn(
                                &t,
                                &format!(
                                    "{label}{}",
                                    if self.java_ui.column == column {
                                        if self.java_ui.descending {
                                            " ↓"
                                        } else {
                                            " ↑"
                                        }
                                    } else {
                                        ""
                                    }
                                ),
                                if self.java_ui.column == column {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Outline
                                },
                            )
                            .when(self.java_ui.column == column, |button| {
                                button.border_1().border_color(t.primary())
                            })
                            .debug_selector(move || format!("java-sort:{label}").into())
                            .text_xs()
                            .on_click(cx.listener(
                                move |v, _: &gpui::ClickEvent, _, cx| {
                                    if v.java_ui.column == column {
                                        v.java_ui.descending = !v.java_ui.descending;
                                    } else {
                                        v.java_ui.column = column;
                                        v.java_ui.descending = false;
                                    }
                                    cx.notify();
                                },
                            ))
                        }),
                    )
                    .child(div().flex_1())
                    .child(
                        btn(
                            &t,
                            if self.preferences_ui.scanning {
                                "Scanning..."
                            } else {
                                "Refresh runtimes"
                            },
                            ButtonVariant::Outline,
                        )
                        .text_xs()
                        .on_click(
                            cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.scan_launcher_java(cx)),
                        ),
                    ),
            );
        let mut rows = div()
            .id("java-chooser-list")
            .max_h(px(260.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        if items.is_empty() {
            rows = rows.child(
                div()
                    .p_3()
                    .text_xs()
                    .text_color(t.muted_foreground())
                    .child("No matching Java runtimes. Browse to select an executable."),
            );
        }
        for java in items {
            let path = java.path.clone();
            let target = target.clone();
            let selector = format!("java-runtime:{path}");
            rows = rows.child(
                list_item(&t, "", false)
                    .aria_label(format!("Select Java {} at {}", java.version, path))
                    .id(format!("java-choice:{path}"))
                    .debug_selector(move || selector.clone().into())
                    .h_auto()
                    .flex_shrink_0()
                    .w_full()
                    .justify_start()
                    .flex_col()
                    .items_start()
                    .py_2()
                    .gap_1()
                    .tab_stop(enabled)
                    .when(!enabled, |row| row.opacity(0.5))
                    .child(format!(
                        "Java {} · {} · {}",
                        java.version, java.architecture, java.vendor
                    ))
                    .child(
                        div()
                            .w_full()
                            .text_xs()
                            .text_color(t.muted_foreground())
                            .text_ellipsis()
                            .child(java.path),
                    )
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if !enabled {
                            return;
                        }
                        if let Some(id) = &target {
                            v.choose_instance_java(id, path.clone(), cx);
                        } else {
                            v.settings_java = FieldState::new(&path);
                            v.save_launcher_settings_form(cx);
                        }
                    })),
            );
        }
        panel = panel.child(rows);
        panel
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::{test_support::Fixture, Tab};
    use gpui::TestAppContext;

    #[gpui::test]
    async fn choosing_a_runtime_persists_global_and_instance_precedence(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["pack"]);
        std::fs::write(fixture.paths.settings_path("pack").unwrap(),
            serde_json::json!({"name":"pack","override_java_location":true,"acceptanceExtension":true}).to_string()).unwrap();
        let (window, root) = fixture.open(cx, |v| {
            v.tab = Tab::Settings;
            v.preferences_ui.installations = Some(
                (8..=29)
                    .map(|version| JavaInfo {
                        path: format!("/acceptance/jdk{version}/bin/java"),
                        version: format!("{version}.0.1"),
                        architecture: "x64".into(),
                        major_version: version,
                        vendor: "Acceptance runtime".into(),
                    })
                    .collect(),
            );
            v.java_ui.search = "jdk21/".into();
        });
        Fixture::click(cx, window, "java-runtime:/acceptance/jdk21/bin/java");
        fixture
            .until(cx, &root, |v| {
                !v.settings_saving && v.settings_save_pending.is_none()
            })
            .await;
        let global = fixture
            .rt
            .block_on(industrialis_backend::settings::load_launcher_settings(
                &fixture.paths,
            ))
            .unwrap();
        assert_eq!(
            global.default_java_path.as_deref(),
            Some("/acceptance/jdk21/bin/java")
        );
        root.update(cx, |v, cx| {
            v.choose_instance_java("pack", "/acceptance/jdk8/bin/java".into(), cx);
            v.flush_instance_settings("pack", cx);
        });
        fixture
            .until(cx, &root, |v| {
                v.data.instances[0].settings.java_path.as_deref()
                    == Some("/acceptance/jdk8/bin/java")
            })
            .await;
        let instance = fixture
            .rt
            .block_on(industrialis_backend::settings::load_instance_settings(
                &fixture.paths,
                "pack",
            ))
            .unwrap();
        assert!(instance.override_java_location);
        assert_eq!(
            instance.java_path.as_deref(),
            Some("/acceptance/jdk8/bin/java")
        );
        assert_eq!(instance.extra_fields["acceptanceExtension"], true);
        assert_eq!(
            fixture
                .rt
                .block_on(industrialis_backend::settings::load_launcher_settings(
                    &fixture.paths
                ))
                .unwrap()
                .default_java_path,
            global.default_java_path
        );
        fixture.cleanup();
    }
    #[test]
    fn search_and_numeric_sort_keep_actual_runtime_metadata() {
        let items: Vec<_> = [
            ("/jdk17", "17.0.12", "amd64"),
            ("/jdk8", "1.8.0_402", "x86"),
            ("/jdk21", "21.0.3", "arm64"),
        ]
        .into_iter()
        .map(|(path, version, architecture)| JavaInfo {
            path: path.into(),
            version: version.into(),
            architecture: architecture.into(),
            major_version: industrialis_backend::java::java_major_version(version).unwrap(),
            vendor: "Temurin".into(),
        })
        .collect();
        let mut chooser = JavaChooser::default();
        assert_eq!(chooser.visible(&items)[2].major_version, 21);
        chooser.search = "ARM64".into();
        assert_eq!(chooser.visible(&items)[0].path, "/jdk21");
        chooser.search.clear();
        chooser.column = Column::Version;
        chooser.descending = true;
        assert_eq!(chooser.visible(&items)[0].major_version, 21);
        chooser.search = "temurin".into();
        assert_eq!(chooser.visible(&items).len(), 3);
    }
}
