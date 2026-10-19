use super::*;
use crate::ui::btn;
use industrialis_backend::icons::IconEntry;
#[derive(Default)]
pub(super) struct IconsUi {
    pub busy: bool,
}
#[derive(Debug, Clone)]
pub(super) struct IconDialog {
    id: String,
    icons: Vec<IconEntry>,
    error: Option<String>,
}
impl LauncherRoot {
    pub(super) fn open_icon_dialog(&mut self, id: String, cx: &mut Context<Self>) {
        if self.instance_is_busy(&id) || self.icons_ui.busy {
            return;
        }
        self.after_instance_settings_saved(
            id.clone(),
            move |v, cx| v.load_icon_dialog(id.clone(), cx),
            cx,
        );
    }
    fn load_icon_dialog(&mut self, id: String, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        self.workspace_ui.menu = None;
        self.icons_ui.busy = true;
        self.dialog = Some(Dialog::Icon(IconDialog {
            id,
            icons: Vec::new(),
            error: None,
        }));
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { industrialis_backend::icons::list_icons(&paths).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.icons_ui.busy = false;
                if let Some(Dialog::Icon(dialog)) = &mut v.dialog {
                    match result {
                        Ok(Ok(icons)) => dialog.icons = icons,
                        Ok(Err(e)) => dialog.error = Some(format!("{e:#}")),
                        Err(e) => dialog.error = Some(e.to_string()),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn open_icon_folder(&mut self, cx: &mut Context<Self>) {
        if self.icons_ui.busy {
            return;
        }
        self.icons_ui.busy = true;
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move { industrialis_backend::icons::open_icons_folder(&paths).await })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.icons_ui.busy = false;
                if let Ok(Err(error)) = result {
                    v.data.status = format!("Could not open icons folder: {error:#}");
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn choose_icon(&mut self, id: String, icon: Option<String>, cx: &mut Context<Self>) {
        if self.icons_ui.busy || self.closing.is_some() {
            return;
        }
        self.icons_ui.busy = true;
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    match icon {
                        Some(icon) => {
                            industrialis_backend::icons::set_icon_from_library(&paths, &id, &icon)
                                .await
                        }
                        None => {
                            industrialis_backend::icons::set_instance_icon(&paths, &id, None).await
                        }
                    }
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.icons_ui.busy = false;
                match result {
                    Ok(Ok(())) => {
                        v.dialog = None;
                        v.refresh(cx);
                    }
                    error => {
                        let message = match error {
                            Ok(Err(e)) => format!("{e:#}"),
                            Err(e) => e.to_string(),
                            _ => unreachable!(),
                        };
                        if let Some(Dialog::Icon(dialog)) = &mut v.dialog {
                            dialog.error = Some(message.clone());
                        }
                        v.data.status = message;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn browse_icon(&mut self, cx: &mut Context<Self>) {
        if self.icons_ui.busy {
            return;
        }
        let picker = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import instance image (up to 4 MB)".into()),
        });
        cx.spawn(async move |view, cx| {
            let selection = picker.await;
            let _ = view.update(cx, |v, cx| match selection {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.first() {
                        v.import_icon(path.clone(), cx);
                    }
                }
                Ok(Ok(None)) => {}
                error => {
                    v.data.status = format!("Could not select image: {error:?}");
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn import_icon(&mut self, source: std::path::PathBuf, cx: &mut Context<Self>) {
        if self.icons_ui.busy
            || self.closing.is_some()
            || !matches!(self.dialog, Some(Dialog::Icon(_)))
        {
            return;
        }
        self.icons_ui.busy = true;
        let paths = self.data.paths.clone();
        let rt = self.data.rt.clone();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    let imported =
                        industrialis_backend::icons::import_icon(&paths, &source).await?;
                    let icons = industrialis_backend::icons::list_icons(&paths).await?;
                    Ok::<_, anyhow::Error>((imported, icons))
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.icons_ui.busy = false;
                if let Some(Dialog::Icon(dialog)) = &mut v.dialog {
                    match result {
                        Ok(Ok((imported, icons))) => {
                            dialog.icons = icons;
                            dialog.error = None;
                            v.data.status =
                                format!("Imported {}. Select it to apply.", imported.label);
                        }
                        Ok(Err(e)) => dialog.error = Some(format!("{e:#}")),
                        Err(e) => dialog.error = Some(e.to_string()),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn icon_dialog(&mut self, dialog: &IconDialog, cx: &mut Context<Self>) -> gpui::Div {
        let mut body = div().flex().flex_col().gap_3();
        if self.icons_ui.busy {
            body = body.child("Loading or saving icons...");
        } else {
            body = body.child(
                div()
                    .id("instance-icon-library")
                    .max_h(px(320.0))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .children(dialog.icons.iter().map(|entry| {
                                let id = dialog.id.clone();
                                let icon = entry.id.clone();
                                let selector = format!("library-icon:{}", entry.id);
                                btn(&self.theme, &entry.label, ButtonVariant::Ghost).id(format!("library-icon:{}",entry.id))
                                    .debug_selector(move || selector.clone().into())
                                    .flex_col()
                                    .w(px(112.0))
                                    .h_auto()
                                    .p_2()
                                    .child(gpui::img(entry.path.clone()).size(px(56.0)))
                                    .child(
                                        div()
                                            .text_xs()
                                            .w_full()
                                            .text_center()
                                            .child(entry.label.clone()),
                                    )
                                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                            v.choose_icon(id.clone(), Some(icon.clone()), cx)
                                        }),
                                    )
                            })),
                    ),
            );
            let id = dialog.id.clone();
            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        btn(&self.theme, "Import image...", ButtonVariant::Secondary)
                            .debug_selector(|| "icon-import".into())
                            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.browse_icon(cx)),
                            ),
                    )
                    .child(
                        btn(&self.theme, "Reset icon", ButtonVariant::Ghost).debug_selector(|| "icon-reset".into()).on_click(cx.listener(move |v, _: &gpui::ClickEvent, _, cx| {
                                v.choose_icon(id.clone(), None, cx)
                            }),
                        ),
                    )
                    .child(
                        btn(&self.theme, "Icons folder", ButtonVariant::Ghost).on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.open_icon_folder(cx)),
                        ),
                    ),
            );
        }
        if let Some(error) = &dialog.error {
            body = body.child(error.clone());
        }
        let footer = div().child(
            btn(&self.theme, "Close", ButtonVariant::Ghost).on_click(cx.listener(|v, _: &gpui::ClickEvent, _, cx| v.close_dialog(cx)),
            ),
        );
        self.dialog_frame("Instance icon", body, footer, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::test_support::Fixture;
    use gpui::TestAppContext;

    #[gpui::test]
    async fn icon_picker_cancel_import_reuse_and_reset_survive_disk_reload(cx: &mut TestAppContext) {
        let fixture = Fixture::new(&["a", "b"]);
        let source = fixture.paths.base().join("acceptance.png");
        std::fs::write(&source, include_bytes!("../../assets/brand.png")).unwrap();
        let (window, root) = fixture.open(cx, |_| {});
        root.update(cx, |v, cx| v.open_icon_dialog("a".into(), cx));
        fixture.until(cx, &root, |v| !v.icons_ui.busy).await;
        Fixture::click(cx, window, "icon-import");
        assert!(cx.did_prompt_for_paths());
        cx.simulate_path_prompt_response(|options| {
            assert!(options.files && !options.multiple && !options.directories);
            None
        });
        cx.run_until_parked();
        assert!(matches!(root.read_with(cx, |v, _| v.dialog.clone()), Some(Dialog::Icon(_))));
        Fixture::click(cx, window, "icon-import");
        cx.simulate_path_prompt_response(|_| Some(vec![source]));
        cx.run_until_parked();
        fixture.until(cx, &root, |v| !v.icons_ui.busy).await;
        let imported = root.read_with(cx, |v, _| match &v.dialog {
            Some(Dialog::Icon(dialog)) => dialog.icons.iter().find(|i| i.id.starts_with("custom-")).unwrap().id.clone(),
            _ => panic!("Icon dialog disappeared"),
        });
        assert_eq!(imported, "custom-acceptance.png");
        Fixture::click(cx, window, "library-icon:custom-acceptance.png");
        fixture.until(cx, &root, |v| !v.icons_ui.busy).await;
        root.update(cx, |v, cx| v.open_icon_dialog("b".into(), cx));
        fixture.until(cx, &root, |v| !v.icons_ui.busy).await;
        Fixture::click(cx, window, "library-icon:custom-acceptance.png");
        fixture.until(cx, &root, |v| !v.icons_ui.busy).await;
        for id in ["a", "b"] {
            let settings = fixture.rt.block_on(industrialis_backend::settings::load_instance_settings(&fixture.paths, id)).unwrap();
            let custom = settings.custom_icon.unwrap();
            assert_eq!(std::fs::read(fixture.paths.instance_dir(id).unwrap().join(custom)).unwrap(), include_bytes!("../../assets/brand.png"));
            assert_eq!(settings.extra_fields["acceptanceExtension"], true);
        }
        root.update(cx, |v, cx| v.open_icon_dialog("a".into(), cx));
        fixture.until(cx, &root, |v| !v.icons_ui.busy).await;
        Fixture::click(cx, window, "icon-reset");
        fixture.until(cx, &root, |v| !v.icons_ui.busy).await;
        assert!(fixture.rt.block_on(industrialis_backend::settings::load_instance_settings(&fixture.paths, "a")).unwrap().custom_icon.is_none());
        assert!(fixture.rt.block_on(industrialis_backend::settings::load_instance_settings(&fixture.paths, "b")).unwrap().custom_icon.is_some());
        fixture.cleanup();
    }
}
