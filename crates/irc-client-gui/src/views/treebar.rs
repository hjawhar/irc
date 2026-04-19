use iced::widget::{button, column, container, scrollable, text};
use iced::{Element, Fill, Length};

use crate::app::{Msg, NetworkInfo, WindowId};

/// Render the left-hand tree bar listing networks and their channels.
pub(crate) fn view<'a>(
    networks: impl Iterator<Item = (&'a irc_client_core::NetworkId, &'a NetworkInfo)>,
    active: Option<WindowId>,
) -> Element<'a, Msg> {
    let mut col = column![].spacing(2).width(Length::Fixed(180.0));

    for (net_id, info) in networks {
        col = col.push(text(&info.name).size(13).style(|theme: &iced::Theme| {
            let palette = theme.extended_palette();
            text::Style {
                color: Some(palette.primary.strong.color),
            }
        }));

        for window in &info.windows {
            let label = String::from_utf8_lossy(&window.target).into_owned();
            let is_active = active == Some(window.id);

            let btn = button(text(label).size(12))
                .on_press(Msg::WindowSelected(window.id))
                .width(Fill);

            let btn = if is_active {
                btn.style(button::primary)
            } else {
                btn.style(button::secondary)
            };

            col = col.push(btn);
        }

        // Implicit borrow of net_id to silence unused warning; the id is
        // used indirectly via info.windows.
        let _ = net_id;
    }

    container(scrollable(col).height(Fill))
        .style(crate::theme::sidebar)
        .height(Fill)
        .into()
}
