use std::collections::HashMap;

use bytes::Bytes;
use iced::widget::{column, container, row, text};
use iced::{Element, Fill, Subscription, Task, Theme};
use tokio::sync::mpsc;
use tracing::{info, warn};

use irc_client_core::{Client, ClientCommand, ClientEvent, NetworkId};

use crate::theme;
use crate::views;

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

/// Unique identifier for a window (network + target pair).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WindowId(u64);

static NEXT_WINDOW_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_window_id() -> WindowId {
    WindowId(NEXT_WINDOW_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

// ---------------------------------------------------------------------------
// Display types
// ---------------------------------------------------------------------------

/// A single message line in the scrollback.
pub(crate) struct DisplayMessage {
    pub(crate) timestamp: String,
    pub(crate) from: String,
    pub(crate) text: String,
    pub(crate) is_action: bool,
}

/// A window pane displaying a channel or query.
pub(crate) struct Window {
    pub(crate) id: WindowId,
    pub(crate) network: NetworkId,
    pub(crate) target: Bytes,
    pub(crate) messages: Vec<DisplayMessage>,
    pub(crate) topic: Option<String>,
    pub(crate) nicks: Vec<String>,
}

/// Summary info for a network, used by the treebar.
pub(crate) struct NetworkInfo {
    pub(crate) name: String,
    pub(crate) windows: Vec<WindowRef>,
}

/// Lightweight reference to a window for the treebar.
pub(crate) struct WindowRef {
    pub(crate) id: WindowId,
    pub(crate) target: Bytes,
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

pub(crate) struct IrcApp {
    /// Sender for IRC commands.
    command_tx: mpsc::Sender<ClientCommand>,

    /// Currently focused window.
    active_window: Option<WindowId>,
    /// All open windows keyed by id.
    windows: HashMap<WindowId, Window>,
    /// Per-network metadata, keyed by `NetworkId`.
    networks: HashMap<NetworkId, NetworkInfo>,

    /// Current input bar contents.
    input_value: String,

    /// Channel list entries accumulated from LIST replies.
    channel_list_entries: Vec<views::channel_list::ListEntry>,
    /// Filter text for channel list.
    channel_list_filter: String,
    /// Whether the channel list is currently loading.
    channel_list_loading: bool,
    /// Whether to show the channel list overlay.
    show_channel_list: bool,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) enum Msg {
    /// An IRC event arrived from the client backend.
    IrcEvent(ClientEvent),
    /// The input field value changed.
    InputChanged(String),
    /// The user pressed Enter in the input field.
    InputSubmit,
    /// The user clicked a window in the treebar.
    WindowSelected(WindowId),
    /// The user clicked Connect (stub).
    #[allow(dead_code)]
    ConnectPressed,
    /// Placeholder for events we don't handle yet.
    #[allow(dead_code)]
    Noop,
    /// Channel list filter text changed.
    ListFilterChanged(String),
    /// User clicked a channel name in the list to join it.
    ListJoinChannel(Bytes),
    /// Close the channel list overlay.
    #[allow(dead_code)]
    ListClose,
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

impl IrcApp {
    pub(crate) fn new() -> (Self, Task<Msg>) {
        let (client, event_rx, command_tx) = Client::new();

        // Park the receiver for the subscription before it runs.
        park_event_rx(event_rx);

        // Spawn the headless client event loop.
        tokio::spawn(async move {
            let mut c = client;
            c.run().await;
        });

        let app = Self {
            command_tx,
            active_window: None,
            windows: HashMap::new(),
            networks: HashMap::new(),
            input_value: String::new(),
            channel_list_entries: Vec::new(),
            channel_list_filter: String::new(),
            channel_list_loading: false,
            show_channel_list: false,
        };

        (app, Task::none())
    }
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

impl IrcApp {
    pub(crate) fn update(&mut self, message: Msg) -> Task<Msg> {
        match message {
            Msg::IrcEvent(event) => {
                self.handle_event(event);
            }
            Msg::InputChanged(value) => {
                self.input_value = value;
            }
            Msg::InputSubmit => {
                self.submit_input();
            }
            Msg::WindowSelected(id) => {
                if self.windows.contains_key(&id) {
                    self.active_window = Some(id);
                }
            }
            Msg::ConnectPressed => {
                info!("connect pressed (stub)");
            }
            Msg::Noop => {}
            Msg::ListFilterChanged(f) => {
                self.channel_list_filter = f;
            }
            Msg::ListJoinChannel(channel) => {
                if let Some(win_id) = self.active_window {
                    if let Some(win) = self.windows.get(&win_id) {
                        let cmd = ClientCommand::Join {
                            network: win.network,
                            channel,
                        };
                        let tx = self.command_tx.clone();
                        tokio::spawn(async move {
                            let _ = tx.send(cmd).await;
                        });
                    }
                }
                self.show_channel_list = false;
            }
            Msg::ListClose => {
                self.show_channel_list = false;
            }
        }
        Task::none()
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn handle_event(&mut self, event: ClientEvent) {
        match event {
            ClientEvent::Connected { network } => {
                self.ensure_network(network);
            }
            ClientEvent::Registered { network, nick } => {
                self.ensure_network(network);
                if let Some(info) = self.networks.get_mut(&network) {
                    info.name = String::from_utf8_lossy(&nick).into_owned();
                }
            }
            ClientEvent::Message {
                network,
                target,
                from,
                text,
            } => {
                let win_id = self.ensure_window(network, target.clone());
                let from_str = String::from_utf8_lossy(&from).into_owned();
                let text_str = String::from_utf8_lossy(&text).into_owned();
                if let Some(win) = self.windows.get_mut(&win_id) {
                    win.messages.push(DisplayMessage {
                        timestamp: now_stamp(),
                        from: from_str.clone(),
                        text: text_str.clone(),
                        is_action: false,
                    });
                }
                // Desktop notification for PMs and highlights.
                let own_nick = self.networks.get(&network).map(|n| n.name.as_str());
                let target_bytes = target.as_ref();
                let is_pm = own_nick.is_some_and(|n| target_bytes == n.as_bytes());
                let is_highlight = !is_pm
                    && own_nick.is_some_and(|n| {
                        text_str
                            .to_ascii_lowercase()
                            .contains(&n.to_ascii_lowercase())
                    });
                if is_pm || is_highlight {
                    crate::notifications::notify_message(&from_str, &text_str, is_pm);
                }
            }
            ClientEvent::Notice {
                network,
                target,
                from,
                text,
            } => {
                let win_id = self.ensure_window(network, target);
                if let Some(win) = self.windows.get_mut(&win_id) {
                    win.messages.push(DisplayMessage {
                        timestamp: now_stamp(),
                        from: String::from_utf8_lossy(&from).into_owned(),
                        text: format!("-NOTICE- {}", String::from_utf8_lossy(&text)),
                        is_action: false,
                    });
                }
            }
            ClientEvent::Join {
                network,
                channel,
                nick,
            } => {
                let win_id = self.ensure_window(network, channel);
                if let Some(win) = self.windows.get_mut(&win_id) {
                    let nick_str = String::from_utf8_lossy(&nick).into_owned();
                    if !win.nicks.contains(&nick_str) {
                        win.nicks.push(nick_str.clone());
                    }
                    win.messages.push(DisplayMessage {
                        timestamp: now_stamp(),
                        from: "***".into(),
                        text: format!("{nick_str} has joined"),
                        is_action: true,
                    });
                }
            }
            ClientEvent::Part {
                network,
                channel,
                nick,
                reason,
            } => {
                let win_id = self.ensure_window(network, channel);
                if let Some(win) = self.windows.get_mut(&win_id) {
                    let nick_str = String::from_utf8_lossy(&nick).into_owned();
                    win.nicks.retain(|n| n != &nick_str);
                    let reason_text = reason
                        .as_ref()
                        .map(|r| format!(" ({})", String::from_utf8_lossy(r)))
                        .unwrap_or_default();
                    win.messages.push(DisplayMessage {
                        timestamp: now_stamp(),
                        from: "***".into(),
                        text: format!("{nick_str} has left{reason_text}"),
                        is_action: true,
                    });
                }
            }
            ClientEvent::TopicChange {
                network,
                channel,
                topic,
            } => {
                let win_id = self.ensure_window(network, channel);
                if let Some(win) = self.windows.get_mut(&win_id) {
                    win.topic = Some(String::from_utf8_lossy(&topic).into_owned());
                }
            }
            ClientEvent::Disconnected { network, reason } => {
                warn!(%reason, "network {} disconnected", network.0);
            }
            ClientEvent::ListEntry {
                channel,
                user_count,
                topic,
                ..
            } => {
                self.channel_list_entries
                    .push(views::channel_list::ListEntry {
                        channel,
                        user_count,
                        topic: String::from_utf8_lossy(&topic).into_owned(),
                    });
            }
            ClientEvent::ListEnd { .. } => {
                self.channel_list_loading = false;
            }
            ClientEvent::NickChange { .. }
            | ClientEvent::Quit { .. }
            | ClientEvent::Numeric { .. }
            | ClientEvent::Error { .. }
            | ClientEvent::DccChatRequest { .. }
            | ClientEvent::DccSendRequest { .. }
            | ClientEvent::DccProgress { .. }
            | ClientEvent::DccComplete { .. } => {
                // Handled minimally for the skeleton.
            }
        }
    }

    fn submit_input(&mut self) {
        let text = std::mem::take(&mut self.input_value);
        if text.is_empty() {
            return;
        }

        let Some(win_id) = self.active_window else {
            return;
        };
        let Some(win) = self.windows.get(&win_id) else {
            return;
        };

        if text.eq_ignore_ascii_case("/list") {
            self.channel_list_entries.clear();
            self.channel_list_filter.clear();
            self.channel_list_loading = true;
            self.show_channel_list = true;
            let cmd = ClientCommand::List {
                network: win.network,
            };
            let tx = self.command_tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(cmd).await;
            });
            return;
        }

        let cmd = ClientCommand::SendPrivmsg {
            network: win.network,
            target: win.target.clone(),
            text: Bytes::from(text),
        };

        let tx = self.command_tx.clone();
        tokio::spawn(async move {
            if tx.send(cmd).await.is_err() {
                warn!("command channel closed");
            }
        });
    }

    // -- helpers --

    fn ensure_network(&mut self, id: NetworkId) {
        self.networks.entry(id).or_insert_with(|| NetworkInfo {
            name: format!("Network {}", id.0),
            windows: Vec::new(),
        });
    }

    fn ensure_window(&mut self, network: NetworkId, target: Bytes) -> WindowId {
        self.ensure_network(network);

        // Check if a window already exists for this network+target.
        for win in self.windows.values() {
            if win.network == network && win.target == target {
                return win.id;
            }
        }

        let id = next_window_id();
        self.windows.insert(
            id,
            Window {
                id,
                network,
                target: target.clone(),
                messages: Vec::new(),
                topic: None,
                nicks: Vec::new(),
            },
        );

        if let Some(info) = self.networks.get_mut(&network) {
            info.windows.push(WindowRef { id, target });
        }

        // Auto-focus the first window.
        if self.active_window.is_none() {
            self.active_window = Some(id);
        }

        id
    }
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

impl IrcApp {
    pub(crate) fn view(&self) -> Element<'_, Msg> {
        let topic_text = self
            .active_window
            .and_then(|id| self.windows.get(&id))
            .and_then(|w| w.topic.as_deref())
            .unwrap_or("No topic");

        let topic_bar = container(text(topic_text).size(12))
            .style(theme::topic_bar)
            .width(Fill)
            .padding(4);

        let treebar = views::treebar::view(self.networks.iter(), self.active_window);

        let (messages, nicks): (&[DisplayMessage], &[String]) = self
            .active_window
            .and_then(|id| self.windows.get(&id))
            .map_or((&[], &[]), |w| (w.messages.as_slice(), w.nicks.as_slice()));

        let scrollback = views::scrollback::view(messages);
        let nicklist = views::nicklist::view(nicks);
        let input = views::input::view(&self.input_value);

        let middle = column![
            topic_bar,
            row![treebar, scrollback, nicklist].height(Fill),
            input
        ];

        if self.show_channel_list {
            let list_view = views::channel_list::view(
                &self.channel_list_entries,
                &self.channel_list_filter,
                self.channel_list_loading,
            );
            return container(list_view).width(Fill).height(Fill).into();
        }

        container(middle).width(Fill).height(Fill).into()
    }
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

impl IrcApp {
    #[allow(clippy::unused_self)]
    pub(crate) fn theme(&self) -> Theme {
        theme::dark()
    }
}

// ---------------------------------------------------------------------------
// Subscription
// ---------------------------------------------------------------------------

impl IrcApp {
    #[allow(clippy::unused_self)]
    pub(crate) fn subscription(&self) -> Subscription<Msg> {
        irc_event_worker()
    }
}

/// Stream that yields `Msg::IrcEvent` from the shared receiver.
///
/// Uses a static slot to pass the receiver from `new()` into the
/// subscription stream, since iced subscriptions don't carry app state.
fn irc_event_worker() -> Subscription<Msg> {
    Subscription::run(|| {
        iced::stream::channel(64, |mut output| async move {
            let Some(mut rx) = EVENT_RX_SLOT.take() else {
                // No receiver available; park forever.
                std::future::pending::<()>().await;
                return;
            };
            while let Some(event) = rx.recv().await {
                // If the UI is gone, stop.
                if output.try_send(Msg::IrcEvent(event)).is_err() {
                    break;
                }
            }
        })
    })
}

/// Global slot to pass the event receiver from construction to subscription.
static EVENT_RX_SLOT: ChannelSlot = ChannelSlot::new();

struct ChannelSlot {
    inner: std::sync::Mutex<Option<mpsc::Receiver<ClientEvent>>>,
}

impl ChannelSlot {
    const fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(None),
        }
    }

    fn store(&self, rx: mpsc::Receiver<ClientEvent>) {
        *self.inner.lock().expect("lock poisoned") = Some(rx);
    }

    fn take(&self) -> Option<mpsc::Receiver<ClientEvent>> {
        self.inner.lock().expect("lock poisoned").take()
    }
}

/// Called once at app construction to park the receiver for the subscription.
pub(crate) fn park_event_rx(rx: mpsc::Receiver<ClientEvent>) {
    EVENT_RX_SLOT.store(rx);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_stamp() -> String {
    // Simple HH:MM timestamp. We avoid pulling in chrono for a skeleton.
    String::from("--:--")
}
