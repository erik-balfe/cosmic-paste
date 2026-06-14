use std::sync::LazyLock;
use std::time::{Duration, Instant};

use cosmic::app::{Core, Task};
use cosmic::iced::core::window;
use cosmic::iced::id::Id as WidgetId;
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Rectangle};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::iced::widget::scrollable::{self, AbsoluteOffset};
use cosmic::theme;
use cosmic::widget::{column, list_column, row, scrollable as scrollable_widget, text};
use cosmic::Element;
use cosmic_paste_core::dbus::client::CosmicPasteProxy;
use cosmic_paste_core::item::format_display_line_middle;
use cosmic_paste_core::format_selection_status;
use cosmic_paste_core::{BUS_NAME, OBJECT_PATH};

use crate::dbus_sub::{self, DbusEvent};
use crate::icons;

pub const APP_ID: &str = "com.system76.CosmicPaste.Applet";

/// Row height used by `list_column` (see libcosmic `list_column::into_element`).
const POPUP_ROW_HEIGHT: u32 = 32;
const POPUP_SCROLL_THRESHOLD: usize = 10;
/// ~10 visible rows; list capped by daemon `max_displayed_history_size`.
const POPUP_SCROLL_VIEWPORT: f32 = 320.0;
/// Middle-truncated label width in the history popup (matches tooltip preview scale).
const POPUP_LABEL_LEN: usize = 72;
/// Stable id so scrollbar position survives popup content rebuilds (DBus refresh, hover).
static POPUP_SCROLL_ID: LazyLock<WidgetId> =
    LazyLock::new(|| WidgetId::new("cosmic-paste-popup-scroll"));

pub struct Flags {
    pub open_popup: bool,
}

pub struct App {
    core: Core,
    popup: Option<Id>,
    history: Vec<(String, String)>,
    popup_labels: Vec<String>,
    active_index: u32,
    tracking: bool,
    daemon_unreachable: bool,
    open_on_init: bool,
    /// Last tray-button anchor (keyboard shortcuts reuse this for popup placement).
    last_popup_anchor: Option<PopupAnchor>,
    last_show_history: Option<Instant>,
}

#[derive(Clone, Copy, Debug)]
pub struct PopupAnchor {
    offset_x: f32,
    offset_y: f32,
    bounds: Rectangle,
}

#[derive(Clone, Debug)]
pub enum Message {
    PopupClosed(Id),
    TogglePopup { anchor: Option<PopupAnchor> },
    SelectIndex(u32),
    SelectDone(Result<(), String>),
    Dbus(DbusEvent),
    Surface(cosmic::surface::Action),
}

impl App {
    fn rebuild_popup_labels(&mut self) {
        self.popup_labels = self
            .history
            .iter()
            .enumerate()
            .map(|(idx, (_, display))| {
                let preview = format_display_line_middle(display, POPUP_LABEL_LEN);
                format!("{} · {preview}", idx + 1)
            })
            .collect();
    }

    fn apply_refreshed(
        &mut self,
        history: Vec<(String, String)>,
        active_index: u32,
        tracking: bool,
    ) -> bool {
        let history_changed = self.history != history;
        self.history = history;
        if history_changed {
            self.rebuild_popup_labels();
        }
        self.active_index = active_index;
        self.tracking = tracking;
        self.daemon_unreachable = false;
        history_changed
    }

    fn tooltip(&self) -> String {
        if self.daemon_unreachable {
            return "Daemon unreachable".into();
        }
        let count = self.history.len();
        if count == 0 {
            return "No history — copy text to record".into();
        }
        if !self.tracking {
            return format!("Paused · {count} items");
        }
        let preview = self
            .history
            .get(self.active_index as usize)
            .map(|(_, display)| display.as_str())
            .unwrap_or("");
        format_selection_status(self.active_index, count as u32, preview)
    }

    fn popup_item_count(&self) -> usize {
        self.popup_labels.len()
    }

    fn default_popup_anchor(&self) -> PopupAnchor {
        let (icon_w, icon_h) = self.core.applet.suggested_size(true);
        let (pad_major, pad_minor) = self.core.applet.suggested_padding(true);
        let (h_pad, v_pad) = if self.core.applet.is_horizontal() {
            (pad_major, pad_minor)
        } else {
            (pad_minor, pad_major)
        };
        PopupAnchor {
            offset_x: 0.0,
            offset_y: 0.0,
            bounds: Rectangle {
                x: h_pad as f32,
                y: v_pad as f32,
                width: icon_w as f32,
                height: icon_h as f32,
            },
        }
    }

    fn popup_anchor(&self) -> PopupAnchor {
        self.last_popup_anchor
            .unwrap_or_else(|| self.default_popup_anchor())
    }

    fn estimated_popup_size(&self) -> Option<(u32, u32)> {
        let count = self.popup_item_count();
        if count == 0 {
            return None;
        }
        let height = if count > POPUP_SCROLL_THRESHOLD {
            POPUP_SCROLL_VIEWPORT as u32 + 16
        } else {
            let rows = count as u32 * POPUP_ROW_HEIGHT;
            let dividers = count.saturating_sub(1) as u32;
            rows.saturating_add(dividers).saturating_add(16)
        };
        Some((360, height))
    }

    fn popup_content(&self) -> Element<'_, Message> {
        if self.daemon_unreachable {
            return text("Clipboard daemon unreachable").into();
        }
        if self.popup_labels.is_empty() {
            return text("No clipboard history yet").into();
        }

        let count = self.popup_labels.len();
        let mut list = list_column::with_capacity(count);
        for (idx, label) in self.popup_labels.iter().enumerate() {
            let active = idx == self.active_index as usize;
            let label_text = if active {
                text::body(label).class(theme::Text::Accent)
            } else {
                text::body(label)
            };
            list = list.add(
                list_column::button(
                    row![label_text.width(Length::Fill)]
                        .align_y(Alignment::Center)
                        .width(Length::Fill),
                )
                .on_press(Message::SelectIndex(idx as u32))
                .selected(active),
            );
        }

        let body = if count > POPUP_SCROLL_THRESHOLD {
            Element::from(
                scrollable_widget(list)
                    .id(POPUP_SCROLL_ID.clone())
                    .height(Length::Fixed(POPUP_SCROLL_VIEWPORT))
                    .width(Length::Fill),
            )
        } else {
            Element::from(list)
        };

        Element::from(
            self.core
                .applet
                .popup_container(column![body].padding([8, 12]).width(Length::Fill)),
        )
    }

    fn show_history_task(&mut self) -> Task<Message> {
        const DEBOUNCE: Duration = Duration::from_millis(300);
        let now = Instant::now();
        if self
            .last_show_history
            .is_some_and(|t| now.duration_since(t) < DEBOUNCE)
        {
            return Task::none();
        }
        self.last_show_history = Some(now);
        self.open_popup_task(Some(self.popup_anchor()))
    }

    fn scroll_popup_to_active_task(&self) -> Task<Message> {
        if self.popup_item_count() <= POPUP_SCROLL_THRESHOLD {
            return Task::none();
        }
        const ROW_STRIDE: f32 = POPUP_ROW_HEIGHT as f32 + 1.0;
        let count = self.popup_item_count();
        let max_y = count.saturating_sub(1) as f32 * ROW_STRIDE;
        let y = (self.active_index as f32 * ROW_STRIDE).min(max_y);
        scrollable::scroll_to(
            POPUP_SCROLL_ID.clone(),
            AbsoluteOffset {
                x: Some(0.0),
                y: Some(y),
            },
        )
    }

    fn open_popup_task(&mut self, anchor: Option<PopupAnchor>) -> Task<Message> {
        let scroll = self.scroll_popup_to_active_task();
        if let Some(id) = self.popup.take() {
            return Task::batch([
                surface_task(destroy_popup(id)),
                surface_task(self.open_popup_action(anchor)),
                scroll,
            ]);
        }
        Task::batch([
            surface_task(self.open_popup_action(anchor)),
            scroll,
        ])
    }

    fn open_popup_action(&self, anchor: Option<PopupAnchor>) -> cosmic::surface::Action {
        let popup_size = self.estimated_popup_size();
        app_popup::<App>(
            move |state: &mut App| {
                let new_id = Id::unique();
                state.popup = Some(new_id);
                let mut popup_settings = state.core.applet.get_popup_settings(
                    state.core.main_window_id().unwrap(),
                    new_id,
                    popup_size,
                    None,
                    None,
                );

                if let Some(anchor) = anchor {
                    popup_settings.positioner.anchor_rect = Rectangle {
                        x: (anchor.bounds.x - anchor.offset_x) as i32,
                        y: (anchor.bounds.y - anchor.offset_y) as i32,
                        width: anchor.bounds.width as i32,
                        height: anchor.bounds.height as i32,
                    };
                }

                popup_settings
            },
            Some(Box::new(|state: &App| {
                state.popup_content().map(cosmic::Action::App)
            })),
        )
    }
}

impl cosmic::Application for App {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Message>) {
        let app = Self {
            core,
            popup: None,
            history: Vec::new(),
            popup_labels: Vec::new(),
            active_index: 0,
            tracking: true,
            daemon_unreachable: false,
            open_on_init: flags.open_popup,
            last_popup_anchor: None,
            last_show_history: None,
        };
        let fetch = Task::future(async {
            match dbus_sub::fetch_state().await {
                Ok(event) => cosmic::Action::App(Message::Dbus(event)),
                Err(()) => cosmic::Action::App(Message::Dbus(DbusEvent::Disconnected)),
            }
        });
        (app, fetch)
    }

    fn subscription(&self) -> cosmic::iced::Subscription<Message> {
        dbus_sub::subscription().map(Message::Dbus)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::TogglePopup { anchor } => {
                if let Some(anchor) = anchor {
                    self.last_popup_anchor = Some(anchor);
                }
                if self.popup.is_some() {
                    let id = self.popup.take().expect("popup id");
                    return surface_task(destroy_popup(id));
                }
                return self.open_popup_task(Some(self.popup_anchor()));
            }
            Message::SelectIndex(index) => {
                self.active_index = index;
                let popup_id = self.popup.take();
                let select = Task::future(async move {
                    let result = select_entry_at_index(index).await;
                    cosmic::Action::App(Message::SelectDone(result))
                });
                if let Some(id) = popup_id {
                    return Task::batch([
                        surface_task(destroy_popup(id)),
                        select,
                    ]);
                }
                return select;
            }
            Message::SelectDone(result) => {
                if let Err(err) = result {
                    tracing::warn!("select failed: {err}");
                }
            }
            Message::Dbus(event) => match event {
                DbusEvent::Refreshed {
                    history,
                    active_index,
                    tracking,
                } => {
                    self.apply_refreshed(history, active_index, tracking);
                    if self.open_on_init {
                        self.open_on_init = false;
                        return self.open_popup_task(Some(self.popup_anchor()));
                    }
                }
                DbusEvent::ActiveIndexChanged { active_index } => {
                    self.active_index = active_index;
                    self.daemon_unreachable = false;
                }
                DbusEvent::ShowHistory => {
                    return self.show_history_task();
                }
                DbusEvent::Disconnected => {
                    self.daemon_unreachable = true;
                }
            },
            Message::Surface(action) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(action),
                ));
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let btn = self
            .core
            .applet
            .icon_button_from_handle(icons::paste_handle())
            .on_press_with_rectangle(|offset, bounds| {
                let anchor = PopupAnchor {
                    offset_x: offset.x,
                    offset_y: offset.y,
                    bounds,
                };
                Message::TogglePopup {
                    anchor: Some(anchor),
                }
            });

        Element::from(self.core.applet.applet_tooltip::<Message>(
            btn,
            self.tooltip(),
            self.popup.is_some(),
            Message::Surface,
            None,
        ))
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        // Popups use `view` + surface actions; this path is unused.
        text("").into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

fn surface_task(action: cosmic::surface::Action) -> Task<Message> {
    cosmic::task::message(cosmic::Action::Cosmic(cosmic::app::Action::Surface(
        action,
    )))
}

async fn select_entry_at_index(index: u32) -> Result<(), String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|err| format!("session bus unavailable: {err}"))?;
    let proxy = CosmicPasteProxy::builder(&conn)
        .destination(BUS_NAME)
        .map_err(|err| err.to_string())?
        .path(OBJECT_PATH)
        .map_err(|err| err.to_string())?
        .build()
        .await
        .map_err(|err| format!("daemon unavailable: {err}"))?;
    proxy
        .select_at_index(index)
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
}