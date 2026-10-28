//! One cached catalog for status, searchable installation and maintenance choices.
use super::*;
use crate::ui::{btn, list_item};
use industrialis_backend::pack_files::{fetch_gtnh_versions, GtnhVersion};
use industrialis_core::pack_version::{get_pack_version_info, PackVersionInfo, PackVersionMeta};
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ReleaseFilter {
    #[default]
    All,
    Stable,
    Beta,
}
impl ReleaseFilter {
    pub(super) fn matches(self, choice: &VersionChoice) -> bool {
        match self {
            Self::All => true,
            Self::Stable => choice.stable,
            Self::Beta => !choice.stable,
        }
    }
}
#[derive(Debug, Clone)]
pub(super) struct VersionChoice {
    pub name: String,
    pub date: String,
    pub stable: bool,
    pub java8: bool,
    pub java17: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::test_support::Fixture;
    use gpui::{size, TestAppContext, VisualTestContext};
    use industrialis_backend::paths::LauncherPaths;
    #[gpui::test]
    fn pack_polling_runs_at_startup_and_hourly_without_duplicate_tasks(cx: &mut TestAppContext) {
        use std::{cell::Cell, rc::Rc};
        let fixture = Fixture::new(&["pack"]);
        let (_, root) = fixture.open(cx, |_| {});
        let ticks = Rc::new(Cell::new(0));
        let extra = Rc::new(Cell::new(0));
        root.update(cx, |v, cx| {
            let ticks = ticks.clone();
            v.start_pack_polling_with(cx, move |_, _| ticks.set(ticks.get() + 1));
            let extra = extra.clone();
            v.start_pack_polling_with(cx, move |_, _| extra.set(extra.get() + 1));
        });
        assert_eq!(ticks.get(), 1);
        assert_eq!(extra.get(), 0);
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(3599));
        cx.run_until_parked();
        assert_eq!(ticks.get(), 1);
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(ticks.get(), 2);
        cx.executor().advance_clock(Duration::from_secs(3600));
        cx.run_until_parked();
        assert_eq!(ticks.get(), 3);
        root.update(cx, |v, _| {
            v.catalog_ui.poll_task = None;
        });
        cx.executor().advance_clock(Duration::from_secs(3600));
        cx.run_until_parked();
        assert_eq!(
            ticks.get(),
            3,
            "Dropping the root-owned task cancels polling"
        );
        fixture.cleanup();
    }
    #[gpui::test]
    fn instance_menu_has_manual_check_and_in_flight_checks_cannot_overlap(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["pack"]);
        let (handle, root) = fixture.open(cx, |v| {
            v.groups_ui.busy = true; // Read-only checks are allowed during group writes.
            v.catalog_ui.loading = true;
        });
        cx.update_window(handle.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        let tile = visual.debug_bounds("instance-tile:pack").unwrap();
        visual.simulate_mouse_down(tile.center(), gpui::MouseButton::Right, Default::default());
        visual.simulate_mouse_up(tile.center(), gpui::MouseButton::Right, Default::default());
        let check = visual.debug_bounds("instance-menu-action:12").unwrap();
        visual.simulate_click(check.center(), Default::default());
        assert!(root.read_with(&visual, |v, _| v.catalog_ui.manual_instance.is_none()));
        assert!(root.read_with(&visual, |v, _| v.workspace_ui.menu.is_some()));
        fixture.cleanup();
    }
    #[gpui::test]
    async fn startup_size_refresh_measures_idle_instances_without_a_toolbar_or_running_game_warning(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&["idle", "running", "updating"]);
        std::fs::write(
            fixture
                .paths
                .instance_dir("idle")
                .unwrap()
                .join(".minecraft/save.bin"),
            vec![1; 8192],
        )
        .unwrap();
        let (window, root) = fixture.open(cx, |v| {
            v.data.running.insert("running".into(), (123, true));
            v.busy.insert("update:updating".into());
        });
        // Same entry point used once the main window is initialized. Repeat it
        // before completion to verify in-flight measurements aren't duplicated.
        root.update(cx, |v, cx| {
            v.refresh_instance_sizes(None, cx);
            v.refresh_instance_sizes(None, cx);
            assert!(v.busy.contains("size:idle"));
            assert!(!v.busy.contains("size:running"));
            assert!(!v.busy.contains("size:updating"));
        });
        fixture
            .until(cx, &root, |v| !v.busy.contains("size:idle"))
            .await;
        root.read_with(cx, |v, _| {
            assert!(v.data.selected_instance(Some("idle")).unwrap().size_bytes >= 8192);
            assert_eq!(
                v.data
                    .selected_instance(Some("running"))
                    .unwrap()
                    .size_bytes,
                0
            );
            assert_eq!(
                v.data.status, "Ready",
                "background measurements don't replace activity status"
            );
        });
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        assert!(visual.debug_bounds("library-refresh-sizes").is_none());
        fixture.cleanup();
    }

    #[gpui::test]
    async fn a_recovered_game_exit_recalculates_size_and_updates_the_library(
        cx: &mut TestAppContext,
    ) {
        use industrialis_backend::running_pids::{GameSession, RunningGamePid};
        let fixture = Fixture::new(&["pack"]);
        std::fs::write(
            fixture
                .paths
                .instance_dir("pack")
                .unwrap()
                .join(".minecraft/save.bin"),
            vec![2; 16384],
        )
        .unwrap();
        let capture_id = "00000000-0000-4000-8000-000000000001";
        let capture = fixture
            .paths
            .instance_dir("pack")
            .unwrap()
            .join(".industrialis-capture")
            .join(capture_id);
        std::fs::create_dir_all(&capture).unwrap();
        std::fs::write(capture.join("stdout.log"), "game stopped\n").unwrap();
        std::fs::write(capture.join("stderr.log"), "").unwrap();
        std::fs::write(capture.join("exit.json"), serde_json::json!({"pid":2147483646u32,"creation_id":"size-fixture","exit_code":7,"ended_at_ms":2000,"post_exit_error":null}).to_string()).unwrap();
        fixture
            .rt
            .block_on(industrialis_backend::running_pids::save_running_game_pids(
                &fixture.paths,
                &HashMap::from([(
                    "pack".into(),
                    RunningGamePid {
                        pid: 2147483646,
                        creation_id: "size-fixture".into(),
                        launcher_session: Some(GameSession {
                            capture_id: capture_id.into(),
                            started_at_ms: 1000,
                            record_time: false,
                            stdout_offset: 0,
                            stderr_offset: 0,
                            extra_fields: Default::default(),
                        }),
                        extra_fields: Default::default(),
                    },
                )]),
            ))
            .unwrap();
        let (_, root) = fixture.open(cx, |v| {
            v.data.running.insert("pack".into(), (2147483646, false));
        });
        root.update(cx, |v, cx| v.monitor_restored_games(cx));
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            cx.run_until_parked();
            cx.executor().advance_clock(Duration::from_millis(100));
            if root.read_with(cx, |v, _| {
                !v.restoring.contains("pack") && v.data.instances[0].size_bytes >= 16384
            }) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "game-exit size didn't reach the library: {}",
                root.read_with(cx, |v, _| v.data.status.clone())
            );
            fixture
                .rt
                .spawn(async { tokio::time::sleep(Duration::from_millis(2)).await })
                .await
                .unwrap();
        }
        assert!(!root.read_with(cx, |v, _| v.data.is_running("pack")));
        let settings = fixture
            .rt
            .block_on(industrialis_backend::settings::load_instance_settings(
                &fixture.paths,
                "pack",
            ))
            .unwrap();
        assert_eq!(
            settings.cached_size_bytes,
            root.read_with(cx, |v, _| v.data.instances[0].size_bytes)
        );
        assert_eq!(settings.extra_fields["acceptanceExtension"], true);
        assert!(!capture.exists());
        fixture.cleanup();
    }

    #[gpui::test]
    async fn size_refresh_updates_the_library_after_external_changes_and_respects_busy_instances(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::new(&["pack"]);
        let (window, root) = fixture.open(cx, |_| {});
        let file = fixture
            .paths
            .instance_dir("pack")
            .unwrap()
            .join(".minecraft/new-data.bin");
        std::fs::write(&file, vec![1; 8192]).unwrap();
        root.update(cx, |v, cx| {
            v.refresh_instance_sizes(Some("pack".into()), cx)
        });
        assert!(root.read_with(cx, |v, _| v.busy.contains("size:pack")));
        fixture
            .until(cx, &root, |v| !v.busy.contains("size:pack"))
            .await;
        let first = root.read_with(cx, |v, _| v.data.instances[0].size_bytes);
        assert!(first >= 8192);
        let saved = fixture
            .rt
            .block_on(industrialis_backend::settings::load_instance_settings(
                &fixture.paths,
                "pack",
            ))
            .unwrap();
        assert_eq!(saved.cached_size_bytes, first);
        assert_eq!(saved.extra_fields["acceptanceExtension"], true);
        std::fs::write(&file, vec![2; 16384]).unwrap();
        root.update(cx, |v, cx| {
            v.busy.insert("update:pack".into());
            v.refresh_instance_sizes(None, cx);
        });
        assert_eq!(
            root.read_with(cx, |v, _| v.data.instances[0].size_bytes),
            first
        );
        assert!(!root.read_with(cx, |v, _| v.busy.contains("size:pack")));
        root.update(cx, |v, cx| {
            v.busy.remove("update:pack");
            v.refresh_instance_sizes(None, cx);
        });
        fixture
            .until(cx, &root, |v| !v.busy.contains("size:pack"))
            .await;
        assert!(root.read_with(cx, |v, _| v.data.instances[0].size_bytes) > first);
        cx.update_window(window.into(), |_, w, cx| w.draw(cx).clear(cx))
            .unwrap();
        fixture.cleanup();
    }
    #[gpui::test]
    fn searches_the_full_catalog_and_blocks_unavailable_java_variants(cx: &mut TestAppContext) {
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let choices: Vec<_> = (1..=12)
            .map(|n| VersionChoice {
                name: format!("release-{n}"),
                stable: n % 2 == 0,
                date: "2026/01/01".into(),
                java8: n == 10,
                java17: n != 10,
            })
            .collect();
        let data = AppData {
            paths: LauncherPaths::new(std::env::temp_dir().join("industrialis-catalog-readonly")),
            rt,
            instances: Vec::new(),
            accounts: Vec::new(),
            settings: Default::default(),
            default_account_id: None,
            running: Default::default(),
            owned_running: Default::default(),
            logs: Default::default(),
            launching: Default::default(),
            status: "Ready".into(),
        };
        let handle = cx.open_window(size(px(800.0), px(600.0)), |_, _| {
            let mut root = LauncherRoot::new(data);
            root.dialog = Some(Dialog::NewInstance(NewInstanceDialog {
                name: FieldState::new("Synthetic"),
                group: FieldState::empty(),
                search: FieldState::empty(),
                filter: ReleaseFilter::All,
                token: 1,
                version: "release-1".into(),
                java_type: "java17+".into(),
                versions: Some(choices),
                versions_loading: false,
                installing: false,
                error: None,
            }));
            root
        });
        let root = handle.root(cx).unwrap();
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        let beta = visual.debug_bounds("install-filter:Beta").unwrap();
        visual.simulate_click(beta.center(), Default::default());
        assert!(visual.debug_bounds("install-version:release-2").is_none());
        // Clicking focuses the control; Space invokes its shared native click.
        visual.simulate_keystrokes("space");
        visual.update(|_,cx|assert!(matches!(&root.read(cx).dialog,Some(Dialog::NewInstance(d)) if d.filter==ReleaseFilter::Beta)));
        let stable = visual.debug_bounds("install-filter:Stable").unwrap();
        visual.simulate_click(stable.center(), Default::default());
        assert!(visual.debug_bounds("install-version:release-1").is_none());
        for _ in 0..30 {
            visual.simulate_keystrokes("tab");
            visual.update(|window, cx| {
                assert!(root
                    .read(cx)
                    .modal_focus
                    .as_ref()
                    .unwrap()
                    .contains_focused(window, cx))
            });
        }
        let field = visual.debug_bounds("field-new-version-search").unwrap();
        visual.simulate_click(field.center(), Default::default());
        visual.simulate_keystrokes("1 0");
        let version = visual
            .debug_bounds("install-version:release-10")
            .expect("Versions after the former eight-row limit must be reachable");
        assert!(visual.debug_bounds("install-version:release-1").is_none());
        visual.simulate_click(version.center(), Default::default());
        visual.update(|_, cx| {
            let Some(Dialog::NewInstance(dialog)) = &root.read(cx).dialog else {
                panic!()
            };
            assert_eq!(dialog.version, "release-10");
            assert_eq!(dialog.java_type, "java8");
        });
        let java17 = visual.debug_bounds("install-java:java17+").unwrap();
        visual.simulate_click(java17.center(), Default::default());
        visual.update(|_, cx| {
            let Some(Dialog::NewInstance(dialog)) = &root.read(cx).dialog else {
                panic!()
            };
            assert_eq!(dialog.java_type, "java8");
        });
        // An invalid draft cannot bypass availability by pressing Enter.
        visual.update(|_, cx| {
            root.update(cx, |v, cx| {
                if let Some(Dialog::NewInstance(dialog)) = &mut v.dialog {
                    dialog.java_type = "java17+".into();
                }
                v.submit_install(cx);
                assert!(v.busy.is_empty());
                assert!(v.data.launching.is_empty());
            })
        });
    }
}
impl VersionChoice {
    pub fn available(&self, java: &str) -> bool {
        if java == "java8" {
            self.java8
        } else {
            self.java17
        }
    }
}
pub(super) fn choices(catalog: &HashMap<String, GtnhVersion>) -> Vec<VersionChoice> {
    let mut choices: Vec<_> = catalog
        .iter()
        .map(|(name, v)| VersionChoice {
            name: name.clone(),
            date: v.release_date.clone(),
            stable: v.title == "Stable release",
            java8: !v.mmc.java8_url.trim().is_empty(),
            java17: !v.mmc.java17_2x_url.trim().is_empty(),
        })
        .collect();
    choices.sort_by(|a, b| {
        industrialis_core::pack_version::parse_release_date_ms(&b.date)
            .cmp(&industrialis_core::pack_version::parse_release_date_ms(
                &a.date,
            ))
            .then_with(|| industrialis_core::natural::compare_natural(&b.name, &a.name))
    });
    choices
}
#[derive(Default)]
pub(super) struct CatalogUi {
    poll_task: Option<gpui::Task<()>>,
    manual_instance: Option<String>,
    pub versions: Option<HashMap<String, PackVersionMeta>>,
    pub loading: bool,
    pub error: Option<String>,
    pub dialog_generation: u64,
}
impl LauncherRoot {
    pub(super) fn load_new_catalog(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::NewInstance(dialog)) = &mut self.dialog else {
            return;
        };
        self.catalog_ui.dialog_generation += 1;
        let token = self.catalog_ui.dialog_generation;
        dialog.token = token;
        dialog.versions_loading = true;
        dialog.error = None;
        let rt = self.data.rt.clone();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = rt.spawn(fetch_gtnh_versions()).await;
            let _ = view.update(cx, |v, cx| {
                let Some(Dialog::NewInstance(dialog)) = &mut v.dialog else {
                    return;
                };
                if dialog.token != token {
                    return;
                }
                dialog.versions_loading = false;
                match result {
                    Ok(Ok(catalog)) => {
                        let versions = choices(&catalog);
                        if !versions.iter().any(|choice| {
                            choice.name == dialog.version && (choice.java8 || choice.java17)
                        }) {
                            dialog.version = versions
                                .iter()
                                .find(|choice| choice.java8 || choice.java17)
                                .map(|choice| choice.name.clone())
                                .unwrap_or_default();
                        }
                        if let Some(choice) =
                            versions.iter().find(|choice| choice.name == dialog.version)
                        {
                            if !choice.available(&dialog.java_type) {
                                dialog.java_type =
                                    if choice.java17 { "java17+" } else { "java8" }.into();
                            }
                        }
                        dialog.versions = Some(versions);
                    }
                    Ok(Err(e)) => {
                        dialog.error = Some(format!("Could not load pack versions: {e:#}"))
                    }
                    Err(e) => dialog.error = Some(format!("Catalog task failed: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn new_instance_dialog(
        &mut self,
        t: &ActiveTheme,
        state: &NewInstanceDialog,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section_title(t, "Name"))
            .child(self.field("new-name", "Instance name", cx))
            .child(section_title(t, "Pack version"))
            .child(self.field("new-version-search", "Search all pack versions", cx));
        body = body.child(
            div().flex().gap_2().children(
                [
                    (ReleaseFilter::All, "All"),
                    (ReleaseFilter::Stable, "Stable"),
                    (ReleaseFilter::Beta, "Beta"),
                ]
                .into_iter()
                .map(|(filter, label)| {
                    btn(
                        t,
                        label,
                        if state.filter == filter {
                            ButtonVariant::Secondary
                        } else {
                            ButtonVariant::Ghost
                        },
                    )
                    .debug_selector(move || format!("install-filter:{label}").into())
                    .text_xs()
                    .on_click(cx.listener(
                        move |v, _: &gpui::ClickEvent, _, cx| {
                            if let Some(Dialog::NewInstance(d)) = &mut v.dialog {
                                d.filter = filter;
                            }
                            cx.notify();
                        },
                    ))
                }),
            ),
        );
        if state.versions_loading {
            body = body.child("Loading pack versions...");
        } else {
            let versions = state.versions.clone().unwrap_or_default();
            let visible: Vec<_> = versions
                .iter()
                .filter(|version| {
                    state.filter.matches(version)
                        && version
                            .name
                            .to_lowercase()
                            .contains(&state.search.text.to_lowercase())
                })
                .collect();
            body = body.child(
                div()
                    .id("install-version-list")
                    .h(px(150.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(visible.iter().map(|version| {
                        let name = version.name.clone();
                        let selector = format!("install-version:{}", version.name);
                        let java8 = version.java8;
                        let java17 = version.java17;
                        list_item(
                            t,
                            &format!(
                                "{}  | {}  | {}",
                                version.name,
                                version.date,
                                match (java8, java17) {
                                    (true, true) => "Java 8 / 17+",
                                    (true, false) => "Java 8",
                                    (false, true) => "Java 17+",
                                    _ => "No archive",
                                }
                            ),
                            version.name == state.version,
                        )
                        .debug_selector(move || selector.clone().into())
                        .w_full()
                        .justify_start()
                        .text_xs()
                        .when(!java8 && !java17, |button| button.opacity(0.5))
                        .on_click(cx.listener(
                            move |v, _: &gpui::ClickEvent, _, cx| {
                                if !java8 && !java17 {
                                    return;
                                }
                                if let Some(Dialog::NewInstance(dialog)) = &mut v.dialog {
                                    dialog.version = name.clone();
                                    dialog.error = None;
                                    if !java17 {
                                        dialog.java_type = "java8".into();
                                    } else if !java8 {
                                        dialog.java_type = "java17+".into();
                                    }
                                }
                                cx.notify();
                            },
                        ))
                    })),
            );
            if visible.is_empty() {
                body = body.child("No matching versions.");
            }
        }
        let selected = state
            .versions
            .as_ref()
            .and_then(|versions| versions.iter().find(|choice| choice.name == state.version));
        body = body
            .child(div().child(format!("Selected: {}", state.version)))
            .child(
                div().flex().gap_2().children(
                    [("java17+", "Java 17+"), ("java8", "Java 8")]
                        .into_iter()
                        .map(|(java, label)| {
                            let available = selected.is_some_and(|choice| choice.available(java));
                            btn(
                                t,
                                label,
                                if state.java_type == java {
                                    ButtonVariant::Primary
                                } else {
                                    ButtonVariant::Secondary
                                },
                            )
                            .debug_selector(move || format!("install-java:{java}").into())
                            .when(!available, |button| button.opacity(0.5).cursor_default())
                            .on_click(cx.listener(
                                move |v, _: &gpui::ClickEvent, _, cx| {
                                    if available {
                                        if let Some(Dialog::NewInstance(dialog)) = &mut v.dialog {
                                            dialog.java_type = java.into();
                                        }
                                        cx.notify();
                                    }
                                },
                            ))
                        }),
                ),
            )
            .child(section_title(t, "Group (optional)"))
            .child(self.field("new-group", "Ungrouped", cx));
        if let Some(error) = &state.error {
            body = body
                .child(error.clone())
                .child(btn(t, "Reload catalog", ButtonVariant::Ghost).on_click(
                    cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.load_new_catalog(cx)),
                ));
        }
        let can_install = !state.installing
            && !state.versions_loading
            && selected.is_some_and(|choice| choice.available(&state.java_type));
        let footer = div()
            .flex()
            .gap_2()
            .child(
                btn(t, "Cancel", ButtonVariant::Ghost)
                    .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.close_dialog(cx))),
            )
            .child(
                btn(t, "Install", ButtonVariant::Primary)
                    .debug_selector(|| "install-submit".into())
                    .when(!can_install, |button| button.opacity(0.5).cursor_default())
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                        if can_install {
                            v.submit_install(cx);
                        }
                    })),
            );
        self.dialog_frame("New instance", body, footer, cx)
    }
    pub(super) fn pack_status_view(
        &self,
        id: &str,
        current: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        use industrialis_core::pack_version::PackUpdateStatus;
        let status = self.pack_status(current);
        if status.status == PackUpdateStatus::UpdateAvailable {
            let target = id.to_string();
            btn(
                &self.theme,
                &format!(
                    "Update available: {}",
                    status.latest_version.unwrap_or_default()
                ),
                ButtonVariant::Ghost,
            )
            .text_xs()
            .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                v.open_maintenance(
                    target.clone(),
                    super::instance_actions::MaintenanceKind::Update,
                    cx,
                );
            }))
            .into_any_element()
        } else {
            div()
                .px_3()
                .py_2()
                .text_xs()
                .text_color(self.theme.muted_foreground())
                .child(match status.status {
                    PackUpdateStatus::UpToDate => "Up to date",
                    _ if self.catalog_ui.loading => "Checking pack updates...",
                    _ => "Pack update status unavailable",
                })
                .into_any_element()
        }
    }
    pub(crate) fn start_pack_polling(&mut self, cx: &mut Context<Self>) {
        self.start_pack_polling_with(cx, Self::refresh_pack_status);
    }
    fn start_pack_polling_with(
        &mut self,
        cx: &mut Context<Self>,
        tick: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) {
        if self.catalog_ui.poll_task.is_some() || self.closing.is_some() {
            return;
        }
        tick(self, cx);
        self.catalog_ui.poll_task = Some(cx.spawn(async move |view, cx| loop {
            cx.background_executor()
                .timer(Duration::from_secs(60 * 60))
                .await;
            let Ok(continue_polling) = view.update(cx, |v, cx| {
                if v.closing.is_some() {
                    return false;
                }
                tick(v, cx);
                true
            }) else {
                break;
            };
            if !continue_polling {
                break;
            }
        }));
    }
    pub(super) fn manual_pack_check(&mut self, id: String, cx: &mut Context<Self>) {
        if self.catalog_ui.loading || self.closing.is_some() {
            return;
        }
        self.catalog_ui.manual_instance = Some(id);
        self.refresh_pack_status(cx);
    }
    pub(crate) fn refresh_pack_status(&mut self, cx: &mut Context<Self>) {
        if self.catalog_ui.loading || self.closing.is_some() {
            return;
        }
        self.catalog_ui.loading = true;
        cx.notify();
        let rt = self.data.rt.clone();
        cx.spawn(async move |view, cx| {
            let result = rt.spawn(fetch_gtnh_versions()).await;
            let _ = view.update(cx, |v, cx| {
                v.catalog_ui.loading = false;
                match result {
                    Ok(Ok(catalog)) => {
                        v.catalog_ui.versions = Some(
                            catalog
                                .into_iter()
                                .map(|(name, v)| {
                                    (
                                        name,
                                        PackVersionMeta {
                                            title: v.title,
                                            release_date: v.release_date,
                                            max_java_version: v.max_java_version,
                                        },
                                    )
                                })
                                .collect(),
                        );
                        v.catalog_ui.error = None;
                    }
                    Ok(Err(e)) => v.catalog_ui.error = Some(format!("{e:#}")),
                    Err(e) => v.catalog_ui.error = Some(e.to_string()),
                }
                if let Some(id) = v.catalog_ui.manual_instance.take() {
                    let name = v
                        .data
                        .selected_instance(Some(&id))
                        .map(super::workspace::display_name)
                        .unwrap_or(id);
                    v.data.status = if v.catalog_ui.error.is_some() {
                        format!("Could not check pack updates for {name}")
                    } else {
                        format!("Checked pack updates for {name}")
                    };
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn pack_status(&self, current: &str) -> PackVersionInfo {
        get_pack_version_info(current, self.catalog_ui.versions.as_ref())
    }
    pub(crate) fn refresh_instance_sizes(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        let ids: Vec<_> = id
            .map(|id| vec![id])
            .unwrap_or_else(|| self.data.instances.iter().map(|i| i.id.clone()).collect())
            .into_iter()
            .filter(|id| !self.instance_is_busy(id))
            .collect();
        if ids.is_empty() {
            return;
        }
        for id in &ids {
            self.busy.insert(format!("size:{id}"));
        }
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let targets = ids.clone();
            let result = rt
                .spawn(async move {
                    industrialis_backend::install::refresh_sizes(&paths, Some(&targets)).await
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                for id in &ids {
                    v.busy.remove(&format!("size:{id}"));
                }
                match result {
                    Ok(Ok(sizes)) => {
                        for instance in &mut v.data.instances {
                            if let Some(bytes) = sizes.get(&instance.id) {
                                instance.size_bytes = *bytes;
                                instance.settings.cached_size_bytes = *bytes;
                            }
                        }
                    }
                    Ok(Err(e)) => v.data.status = format!("Could not refresh sizes: {e:#}"),
                    Err(e) => v.data.status = format!("Size refresh failed: {e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
