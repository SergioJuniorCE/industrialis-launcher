//! AccountsTab parity: inline offline creation, Microsoft device login,
//! persisted defaults, and removal. Auth tokens stay in the backend.

use std::future::Future;
use std::sync::Arc;

use base64::Engine as _;
use gpui::{
    div, img, prelude::*, px, ClipboardItem, Context, Div, Image, ImageFormat, MouseDownEvent,
    Window,
};
use industrialis_backend::accounts::{self, account_to_info, AccountInfo, DeviceCodeInfo};
use industrialis_backend::settings::load_launcher_settings;
use industrialis_core::instance_settings::InstanceSettings;
use industrialis_core::launcher_settings::resolve_default_account_id;

use super::{LauncherRoot, Tab};
use crate::input::FieldState;
use crate::theme::ActiveTheme;
use crate::ui::{badge, btn, card, BadgeKind, ButtonVariant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum AccountOperation {
    CreateOffline,
    MicrosoftLogin,
    SetDefault,
    Remove,
}

pub(super) struct AccountsState {
    pub offline_username: FieldState,
    operation: Option<AccountOperation>,
    device_code: Option<DeviceCodeInfo>,
    code_copied: bool,
    error: Option<String>,
    pub redirect_instance: Option<String>,
    pub(super) switcher_open: bool,
}

impl Default for AccountsState {
    fn default() -> Self {
        Self {
            offline_username: FieldState::empty(),
            operation: None,
            device_code: None,
            code_copied: false,
            error: None,
            redirect_instance: None,
            switcher_open: false,
        }
    }
}

impl AccountsState {
    pub(super) fn is_busy(&self) -> bool {
        self.operation.is_some()
    }
}

fn license_status(account: &AccountInfo) -> Option<&'static str> {
    if account.account_type == "offline" {
        None
    } else if account.can_play_minecraft == Some(false) {
        Some("Does not own Minecraft Java Edition")
    } else if account.owns_minecraft == Some(false) && account.can_play_minecraft == Some(true) {
        Some("Plays via PC Game Pass")
    } else {
        None
    }
}

fn account_label(account: &AccountInfo) -> &str {
    if !account.username.trim().is_empty() {
        &account.username
    } else if account.account_type == "offline" {
        "Offline account"
    } else {
        "Microsoft account"
    }
}

fn account_type_label(account: &AccountInfo) -> &'static str {
    if account.account_type == "offline" {
        "Offline"
    } else {
        "Microsoft"
    }
}

/// Mirrors App.tsx's resolveLaunchAccount, including missing explicit ids.
pub(super) fn resolve_launch_account<'a>(
    accounts: &'a [AccountInfo],
    default: Option<&str>,
    settings: &InstanceSettings,
) -> Option<&'a AccountInfo> {
    let requested = if settings.override_account {
        settings
            .account_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .or(default)
    } else {
        default
    };
    match requested {
        Some(id) => accounts.iter().find(|account| account.id == id),
        None if accounts.len() == 1 => accounts.first(),
        None => None,
    }
}

/// The same face + hat sprite crop as Electron's SkinFace component.
fn avatar(theme: &ActiveTheme, account: &AccountInfo, side: f32) -> Div {
    let mut face = div()
        .relative()
        .w(px(side))
        .h(px(side))
        .flex_shrink_0()
        .overflow_hidden()
        .rounded_sm();
    if let Some(bytes) = account
        .skin_png_base64
        .as_ref()
        .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
    {
        let image = Arc::new(Image::from_bytes(ImageFormat::Png, bytes));
        face = face
            .child(
                img(image.clone())
                    .absolute()
                    .left(px(-side))
                    .top(px(-side))
                    .w(px(side * 8.0)),
            )
            .child(
                img(image)
                    .absolute()
                    .left(px(-side * 5.0))
                    .top(px(-side))
                    .w(px(side * 8.0)),
            );
    } else {
        face = face
            .bg(theme.secondary())
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(10.0))
            .child(
                account
                    .username
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string(),
            );
    }
    face
}

fn panel(theme: &ActiveTheme) -> Div {
    card(theme)
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
}

#[track_caller]
fn action(
    theme: &ActiveTheme,
    label: &str,
    variant: ButtonVariant,
    enabled: bool,
) -> gpui::Stateful<Div> {
    btn(theme, label, variant)
        .text_xs()
        .tab_stop(enabled)
        .when(!enabled, |el| el.opacity(0.5).cursor_default())
}

impl LauncherRoot {
    pub(super) fn ensure_launch_account(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(instance) = self.data.selected_instance(Some(id)) else {
            return false;
        };
        if resolve_launch_account(
            &self.data.accounts,
            self.data.default_account_id.as_deref(),
            &instance.settings,
        )
        .is_some()
        {
            return true;
        }
        self.accounts_ui.redirect_instance = Some(instance.settings.name.clone());
        self.tab = Tab::Accounts;
        cx.notify();
        false
    }

    /// Serialize UI account mutations so two read/modify/write operations
    /// cannot silently replace one another in accounts.json.
    fn run_account_operation(
        &mut self,
        operation: AccountOperation,
        work: impl Future<Output = anyhow::Result<Option<AccountInfo>>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.accounts_ui.is_busy() {
            return;
        }
        self.refresh_generation += 1;
        self.accounts_ui.operation = Some(operation);
        self.accounts_ui.error = None;
        self.accounts_ui.switcher_open = false;
        cx.notify();
        let rt = self.data.rt.clone();
        let paths = self.data.paths.clone();
        cx.spawn(async move |view, cx| {
            let result = rt
                .spawn(async move {
                    let outcome = async {
                        let added = work.await?;
                        if let Some(account) = added.as_ref() {
                            accounts::set_default_account(&paths, Some(&account.id)).await?;
                        }
                        Ok::<_, anyhow::Error>(added)
                    }
                    .await;
                    // Reload even on partial failure (for example, an account was
                    // written but saving its default failed).
                    let snapshot = tokio::try_join!(
                        accounts::load_accounts(&paths),
                        load_launcher_settings(&paths)
                    );
                    (outcome, snapshot)
                })
                .await;
            let _ = view.update(cx, |v, cx| {
                v.refresh_generation += 1;
                v.accounts_ui.operation = None;
                v.accounts_ui.device_code = None;
                v.accounts_ui.code_copied = false;
                match result {
                    Ok((outcome, snapshot)) => {
                        match snapshot {
                            Ok((accounts, settings)) => {
                                v.data.accounts = accounts.iter().map(account_to_info).collect();
                                v.data.default_account_id =
                                    resolve_default_account_id(&settings).map(str::to_string);
                                v.data.settings.default_account_id = settings.default_account_id;
                                v.data.settings.active_account_id = settings.active_account_id;
                            }
                            Err(e) => {
                                v.accounts_ui.error =
                                    Some(format!("Could not reload accounts: {e:#}"))
                            }
                        }
                        match outcome {
                            Ok(Some(account)) => {
                                v.data.status =
                                    format!("Added account {}", account_label(&account));
                                v.accounts_ui.redirect_instance = None;
                                if operation == AccountOperation::CreateOffline {
                                    v.accounts_ui.offline_username = FieldState::empty();
                                }
                            }
                            Ok(None) => v.data.status = "Accounts updated".to_string(),
                            Err(e) => v.accounts_ui.error = Some(format!("{e:#}")),
                        }
                    }
                    Err(e) => v.accounts_ui.error = Some(format!("Account task failed: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn set_default_account(&mut self, id: String, cx: &mut Context<Self>) {
        if self.data.default_account_id.as_ref() == Some(&id) {
            return;
        }
        let paths = self.data.paths.clone();
        self.run_account_operation(
            AccountOperation::SetDefault,
            async move {
                accounts::set_default_account(&paths, Some(&id)).await?;
                Ok(None)
            },
            cx,
        );
    }

    fn remove_account(&mut self, id: String, cx: &mut Context<Self>) {
        let paths = self.data.paths.clone();
        self.run_account_operation(
            AccountOperation::Remove,
            async move {
                accounts::remove_account(&paths, &id).await?;
                Ok(None)
            },
            cx,
        );
    }

    pub(super) fn submit_offline_account(&mut self, cx: &mut Context<Self>) {
        if self.accounts_ui.is_busy() {
            return;
        }
        let username =
            match accounts::validate_offline_username(&self.accounts_ui.offline_username.text) {
                Ok(username) => username,
                Err(_) => {
                    self.accounts_ui.error = Some(
                        "Username must be 1-16 characters: letters, numbers, and underscores only."
                            .into(),
                    );
                    cx.notify();
                    return;
                }
            };
        let paths = self.data.paths.clone();
        self.run_account_operation(
            AccountOperation::CreateOffline,
            async move {
                accounts::create_offline_account(&paths, &username)
                    .await
                    .map(Some)
            },
            cx,
        );
    }

    fn start_microsoft_login(&mut self, cx: &mut Context<Self>) {
        if self.accounts_ui.is_busy() {
            return;
        }
        let paths = self.data.paths.clone();
        let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
        self.run_account_operation(
            AccountOperation::MicrosoftLogin,
            async move {
                accounts::start_microsoft_login(&paths, move |info| {
                    let _ = send.send(info);
                })
                .await
                .map(Some)
            },
            cx,
        );
        cx.spawn(async move |view, cx| {
            while let Some(info) = receive.recv().await {
                if view
                    .update(cx, |v, cx| {
                        if v.accounts_ui.operation != Some(AccountOperation::MicrosoftLogin) {
                            return;
                        }
                        cx.write_to_clipboard(ClipboardItem::new_string(info.user_code.clone()));
                        v.accounts_ui.device_code = Some(info);
                        v.accounts_ui.code_copied = true;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn accounts_view(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme.clone();
        let enabled = !self.accounts_ui.is_busy();
        let mut content = div().id("accounts-scroll").flex_1().min_h_0().overflow_y_scroll()
            .flex().flex_col().gap_4().w_full().max_w(px(672.0)).px_4().pt(px(20.0)).pb_4()
            .child(div().flex().flex_col().gap_1()
                .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child("Accounts"))
                .child(div().text_xs().text_color(t.muted_foreground())
                    .child("Click the star on an account to use it for every launch. Override per instance in instance settings if needed.")));
        if let Some(instance) = &self.accounts_ui.redirect_instance {
            content = content.child(
                panel(&t)
                    .text_xs()
                    .child(format!("Default account required to launch {instance}"))
                    .child(div().text_color(t.muted_foreground()).child(
                        "Add an account below and click its star, then return to Instances.",
                    )),
            );
        }
        if self.data.accounts.is_empty() && self.accounts_ui.redirect_instance.is_none() {
            content = content.child(
                div()
                    .text_xs()
                    .text_color(t.muted_foreground())
                    .child("No accounts yet."),
            );
        }
        if !self.data.accounts.is_empty() {
            content = content.child(
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(t.border())
                    .overflow_hidden()
                    .children(
                        self.data
                            .accounts
                            .iter()
                            .enumerate()
                            .map(|(index, account)| {
                                let selected =
                                    self.data.default_account_id.as_ref() == Some(&account.id);
                                let select_id = account.id.clone();
                                let remove_id = account.id.clone();
                                let mut identity = div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .text_xs()
                                            .child(account_label(account).to_string())
                                            .child(
                                                badge(
                                                    &t,
                                                    account_type_label(account),
                                                    BadgeKind::Normal,
                                                )
                                                .text_size(px(10.0)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(t.muted_foreground())
                                            .child(account.uuid.clone()),
                                    );
                                if let Some(status) = license_status(account) {
                                    identity = identity.child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(
                                                if account.can_play_minecraft == Some(false) {
                                                    gpui::rgb(0xf59e0b).into()
                                                } else {
                                                    t.muted_foreground()
                                                },
                                            )
                                            .child(status),
                                    );
                                }
                                div()
                                    .id(format!("account-row:{}", account.id))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px(px(10.0))
                                    .py(px(6.0))
                                    .when(index > 0, |el| el.border_t_1().border_color(t.border()))
                                    .bg(if selected { t.muted() } else { t.background() })
                                    .child(avatar(&t, account, 24.0))
                                    .child(identity)
                                    .child(
                                        action(
                                            &t,
                                            "",
                                            ButtonVariant::Ghost,
                                            enabled,
                                        )
                                        .id(format!("account-default:{}", account.id))
                                        .aria_label(format!("Set {} as default account", account_label(account)))
                                        .aria_toggled(if selected { gpui::accesskit::Toggled::True } else { gpui::accesskit::Toggled::False })
                                        .size(px(24.0)).px_0().child(crate::ui::icon(if selected { "star-filled" } else { "star" }, 14.0))
                                        .text_color(if selected {
                                            gpui::rgb(0xfbbf24).into()
                                        } else {
                                            t.muted_foreground()
                                        })
                                        .on_click(cx.listener(
                                                move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                                    v.set_default_account(select_id.clone(), cx);
                                                },
                                            ),
                                        ),
                                    )
                                    .child(
                                        action(&t, "Remove", ButtonVariant::Danger, enabled).h(px(24.0)).px_2()
                                            .id(format!("account-remove:{}", account.id))
                                            .aria_label(format!("Remove account {}", account_label(account)))
                                            .on_click(cx.listener(
                                                move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                                    v.remove_account(remove_id.clone(), cx);
                                                },
                                            ),
                                        ),
                                    )
                            }),
                    ),
            );
        }
        content = content.child(panel(&t)
            .child(div().text_xs().child("Microsoft account"))
            .child(div().text_size(px(11.0)).text_color(t.muted_foreground())
                .child("Sign in with your Microsoft account to play online. Ownership is verified via the Mojang API after login."))
            .child(action(&t, if self.accounts_ui.operation == Some(AccountOperation::MicrosoftLogin) { "Logging in…" } else { "Sign in with Microsoft" }, ButtonVariant::Primary, enabled)
                .flex().justify_center().w_full()
                .on_click(cx.listener(|v, _: &gpui::ClickEvent, _: &mut Window, cx| v.start_microsoft_login(cx)))));
        let create_enabled = enabled && !self.accounts_ui.offline_username.text.trim().is_empty();
        let input = self
            .field("account-name", "Steve", cx)
            .flex_1()
            .min_w_0()
            .when(!enabled, |el| el.opacity(0.5));
        content = content.child(panel(&t)
            .child(div().text_xs().child("Offline account"))
            .child(div().text_size(px(11.0)).text_color(t.muted_foreground())
                .child("Play without signing in. Letters, numbers, underscores (up to 16 characters)."))
            .child(div().flex().gap_2().child(input)
                .child(action(&t, if self.accounts_ui.operation == Some(AccountOperation::CreateOffline) { "Creating…" } else { "Create" }, ButtonVariant::Secondary, create_enabled)
                    .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                        if create_enabled { v.submit_offline_account(cx); }
                    })))));
        if let Some(info) = self.accounts_ui.device_code.clone() {
            let uri = info.verification_uri.clone();
            let code = info.user_code.clone();
            content = content.child(panel(&t)
                .child(div().text_xs().child("Complete Microsoft sign-in"))
                .child(div().text_size(px(11.0)).text_color(t.muted_foreground()).child("Paste the code below at"))
                .child(btn(&t,&info.verification_uri,ButtonVariant::Ghost).text_xs().underline()
                    .on_click(cx.listener(move |_, _: &gpui::ClickEvent, _: &mut Window, cx| cx.open_url(&uri))))
                .child(div().flex().items_center().gap_2()
                    .child(div().flex_1().flex().justify_center().p_2().rounded_md().bg(t.muted()).text_xl().child(info.user_code))
                    .child(action(&t, if self.accounts_ui.code_copied { "✓ Copied" } else { "Copy code" }, ButtonVariant::Secondary, true)
                        .on_click(cx.listener(move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                            v.accounts_ui.code_copied = true;
                            cx.notify();
                        }))))
                .child(div().text_size(px(10.0)).text_color(t.muted_foreground())
                    .child(if self.accounts_ui.code_copied { "The code was copied automatically. Paste it into the Microsoft page." } else { "Use Copy code to copy the code." }))
                .child(div().text_size(px(10.0)).text_color(t.muted_foreground()).child(info.message)));
        }
        if let Some(error) = &self.accounts_ui.error {
            content = content.child(
                div()
                    .text_xs()
                    .text_color(t.destructive())
                    .child(error.clone()),
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .justify_start()
            .child(content)
    }

    pub(super) fn account_switcher(
        &self,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let t = &self.theme;
        let selected = self
            .data
            .accounts
            .iter()
            .find(|a| Some(&a.id) == self.data.default_account_id.as_ref());
        let mut toggle = action(t, "", ButtonVariant::Ghost, true)
            .aria_label(format!("Default account: {}", selected.map(account_label).unwrap_or("No default")))
            .id("toolbar-account")
            .debug_selector(|| "toolbar-account".into())
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(28.0))
            .px(px(10.0))
            .max_w(px(180.0));
        if let Some(account) = selected {
            toggle = toggle.child(avatar(t, account, 20.0));
        } else {
            toggle = toggle.child(crate::ui::icon("user-circle", 16.0));
        }
        if !compact {
            toggle = toggle.child(
                div().min_w_0().text_ellipsis().text_xs().child(
                    selected
                        .map(account_label)
                        .unwrap_or("No default")
                        .to_string(),
                ),
            );
        }
        toggle = toggle
            .child(crate::ui::icon("chevron-down", 12.0))
            .on_click(cx.listener(|v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                v.accounts_ui.switcher_open = !v.accounts_ui.switcher_open;
                cx.notify();
            }));
        let mut container = div().id("account-switcher").relative().child(toggle);
        if self.accounts_ui.switcher_open {
            let mut menu = div()
                .absolute()
                .right_0()
                .top(px(32.0))
                .w(px(256.0))
                .p_1()
                .rounded_md()
                .bg(t.popover())
                .border_1()
                .border_color(t.border())
                .shadow_lg();
            if self.data.accounts.is_empty() {
                menu = menu.child(
                    div()
                        .p_2()
                        .text_xs()
                        .text_color(t.muted_foreground())
                        .child("No accounts yet. Add one to launch instances."),
                );
            } else {
                menu = menu.child(
                    div()
                        .id("account-options")
                        .max_h(px(256.0))
                        .overflow_y_scroll()
                        .children(self.data.accounts.iter().map(|account| {
                            let id = account.id.clone();
                            let is_default = Some(&id) == self.data.default_account_id.as_ref();
                            let mut row = action(
                                t,
                                account_label(account),
                                ButtonVariant::Ghost,
                                !self.accounts_ui.is_busy(),
                            )
                            .id(format!("account-choice:{id}"))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(avatar(t, account, 24.0))
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .child(account_label(account).to_string())
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(t.muted_foreground())
                                            .child(account_type_label(account)),
                                    ),
                            );
                            if is_default {
                                row =
                                    row.bg(t.muted())
                                        .child(badge(t, "Default", BadgeKind::Normal));
                            }
                            row.on_click(cx.listener(
                                move |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                    v.accounts_ui.switcher_open = false;
                                    v.set_default_account(id.clone(), cx);
                                    cx.notify();
                                },
                            ))
                        })),
                );
            }
            menu = menu.child(
                div()
                    .mt_1()
                    .pt_1()
                    .border_t_1()
                    .border_color(t.border())
                    .child(
                        action(t, "Manage accounts…", ButtonVariant::Ghost, true)
                            .w_full()
                            .on_click(cx.listener(
                                |v, _: &gpui::ClickEvent, _: &mut Window, cx| {
                                    v.accounts_ui.switcher_open = false;
                                    v.tab = Tab::Accounts;
                                    cx.notify();
                                },
                            )),
                    ),
            );
            container = container
                .child(gpui::deferred(menu).with_priority(1))
                .on_mouse_down_out(cx.listener(|v, _: &MouseDownEvent, _: &mut Window, cx| {
                    v.accounts_ui.switcher_open = false;
                    cx.notify();
                }));
        }
        container
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str) -> AccountInfo {
        AccountInfo {
            id: id.into(),
            username: id.into(),
            uuid: id.into(),
            account_type: "offline".into(),
            skin_png_base64: None,
            owns_minecraft: None,
            can_play_minecraft: None,
        }
    }

    #[test]
    fn launch_redirect_respects_explicit_missing_accounts_and_overrides() {
        let accounts = vec![account("steve"), account("alex")];
        let mut settings = InstanceSettings::default();
        assert!(resolve_launch_account(&accounts, None, &settings).is_none());
        assert_eq!(
            resolve_launch_account(&accounts, Some("steve"), &settings)
                .unwrap()
                .id,
            "steve"
        );
        settings.override_account = true;
        settings.account_id = Some("alex".into());
        assert_eq!(
            resolve_launch_account(&accounts, Some("steve"), &settings)
                .unwrap()
                .id,
            "alex"
        );
        settings.account_id = Some("removed".into());
        assert!(resolve_launch_account(&accounts, Some("steve"), &settings).is_none());
        let lone = &accounts[..1];
        settings.override_account = false;
        assert!(resolve_launch_account(lone, Some("removed"), &settings).is_none());
        assert_eq!(
            resolve_launch_account(lone, None, &settings).unwrap().id,
            "steve"
        );
        assert!(resolve_launch_account(&[], None, &settings).is_none());
    }
}
