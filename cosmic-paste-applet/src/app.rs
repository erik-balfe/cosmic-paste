use cosmic::app::{Core, Task};
use cosmic::iced::core::window;
use cosmic::iced::window::Id;
use cosmic::iced::{Length, Rectangle};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::{list_column, text};
use cosmic::Element;
use cosmic_paste_core::dbus::client::CosmicPasteProxy;
use cosmic_paste_core::{BUS_NAME, OBJECT_PATH};

use crate::dbus_sub::{self, DbusEvent};
use crate::icons;

pub const APP_ID: &str = "com.system76.CosmicPaste.Applet";

pub struct Flags {
    pub open_popup: bool,
}

pub struct App {
    core: Core,
    popup: Option<Id>,
    history: Vec<(String, String)>,
    active_index: u32,
    tracking: bool,
    daemon_unreachable: bool,
    open_on_init: bool,
}

#[derive(Clone, Debug)]
pub enum Message {
    PopupClosed(Id),
    OpenPopup,
    Select(String),
    SelectDone(Result<(), String>),
    Dbus(DbusEvent),
    Surface(cosmic::surface::Action),
}

impl App {
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
        let index = self.active_index.saturating_add(1);
        let preview = self
            .history
            .get(self.active_index as usize)
            .map(|(_, display)| display.as_str())
            .unwrap_or("");
        format!("{index}/{count}: {preview}")
    }

    fn popup_content(&self) -> Element<'_, Message> {
        if self.daemon_unreachable {
            return text("Clipboard daemon unreachable").into();
        }
        if self.history.is_empty() {
            return text("No clipboard history yet").into();
        }

        let mut column = list_column();
        for (idx, (uuid, display)) in self.history.iter().enumerate() {
            let active = idx == self.active_index as usize;
            let label = if active {
                format!("▸ {display}")
            } else {
                display.clone()
            };
            column = column.add(
                cosmic::widget::button::text(label)
                    .width(Length::Fill)
                    .on_press(Message::Select(uuid.clone())),
            );
        }
        Element::from(self.core.applet.popup_container(column))
    }

    fn open_popup_message(&self) -> cosmic::surface::Action {
        app_popup::<App>(
            move |state: &mut App| {
                let new_id = Id::unique();
                state.popup = Some(new_id);
                state.core.applet.get_popup_settings(
                    state.core.main_window_id().unwrap(),
                    new_id,
                    None,
                    None,
                    None,
                )
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
            active_index: 0,
            tracking: true,
            daemon_unreachable: false,
            open_on_init: flags.open_popup,
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
            Message::OpenPopup => {
                if self.popup.is_none() {
                    return open_popup_task(self);
                }
            }
            Message::Select(uuid) => {
                return Task::future(async move {
                    let result = select_entry(&uuid).await;
                    cosmic::Action::App(Message::SelectDone(result))
                });
            }
            Message::SelectDone(result) => {
                if let Err(err) = result {
                    tracing::warn!("select failed: {err}");
                } else if let Some(id) = self.popup {
                    return surface_task(destroy_popup(id));
                }
            }
            Message::Dbus(event) => match event {
                DbusEvent::Refreshed {
                    history,
                    active_index,
                    tracking,
                } => {
                    self.history = history;
                    self.active_index = active_index;
                    self.tracking = tracking;
                    self.daemon_unreachable = false;
                    if self.open_on_init {
                        self.open_on_init = false;
                        return Task::done(cosmic::Action::App(Message::OpenPopup));
                    }
                }
                DbusEvent::ShowHistory => {
                    return Task::done(cosmic::Action::App(Message::OpenPopup));
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
        let have_popup = self.popup;
        let btn = self
            .core
            .applet
            .icon_button_from_handle(icons::paste_handle())
            .on_press_with_rectangle(move |offset, bounds| {
                if let Some(id) = have_popup {
                    Message::Surface(destroy_popup(id))
                } else {
                    Message::Surface(app_popup::<App>(
                        move |state: &mut App| {
                            let new_id = Id::unique();
                            state.popup = Some(new_id);
                            let mut popup_settings = state.core.applet.get_popup_settings(
                                state.core.main_window_id().unwrap(),
                                new_id,
                                None,
                                None,
                                None,
                            );

                            popup_settings.positioner.anchor_rect = Rectangle {
                                x: (bounds.x - offset.x) as i32,
                                y: (bounds.y - offset.y) as i32,
                                width: bounds.width as i32,
                                height: bounds.height as i32,
                            };
                            popup_settings
                        },
                        Some(Box::new(|state: &App| {
                            state.popup_content().map(cosmic::Action::App)
                        })),
                    ))
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
        "oops".into()
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

fn open_popup_task(state: &App) -> Task<Message> {
    surface_task(state.open_popup_message())
}

async fn select_entry(uuid: &str) -> Result<(), String> {
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
        .select(uuid)
        .await
        .map_err(|err| err.to_string())
}