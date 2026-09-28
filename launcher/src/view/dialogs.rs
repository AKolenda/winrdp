// SPDX-License-Identifier: AGPL-3.0-only
//! Computer editing, sign-in and confirmation dialogs.
use iced::alignment::Vertical;
use iced::widget::{Space, button, column, row};
use iced::{Fill, Padding};

use crate::app::{ComputerForm, Confirm, FORM_NAME, FORM_USERNAME, Message, PASSWORD, PasswordForm};
use crate::style::{self, Tokens};

use super::Element;
use super::widgets::{
    check_row, connection_options, dialog_frame, field_label, footer_button, input, label, resolution_picker, strong,
};

pub(super) fn computer<'a>(t: Tokens, form: &'a ComputerForm) -> Element<'a> {
    let mut body = column![
        field_label(
            t,
            "Name (optional)",
            input(t, "Office PC", &form.name, Message::FormName)
                .id(FORM_NAME)
                .on_submit(Message::FormSave)
        ),
        field_label(
            t,
            "Address",
            input(t, "192.168.1.24", &form.address, Message::FormAddress).on_submit(Message::FormSave)
        ),
        field_label(
            t,
            "Username",
            input(t, "Alex or WORK\\Alex", &form.username, Message::FormUsername)
                .id(FORM_USERNAME)
                .on_submit(Message::FormSave)
        ),
    ]
    .extend(connection_options(t, form.options, Message::FormConnectionOption))
    .push(resolution_picker(t, form.options.resolution, Message::FormResolution))
    .push(check_row(t, "Favourite", form.favourite, Message::FormFavourite))
    .spacing(12)
    .padding(Padding {
        top: 14.0,
        right: 22.0,
        bottom: 18.0,
        left: 22.0,
    });
    if form.saved_password {
        body = body.push(
            row![
                label("A password is remembered for this computer.", 12.5, t.mute),
                Space::new().width(Fill),
                button(label("Forget", 12.5, t.accent))
                    .padding(0)
                    .style(style::link(t))
                    .on_press(Message::FormForgetPassword),
            ]
            .align_y(Vertical::Center),
        );
    }
    if let Some(error) = &form.error {
        body = body.push(label(error.as_str(), 12.0, t.danger));
    }
    let remove: Element<'_> = if form.editing.is_some() {
        button(label("Remove", 13.0, t.danger))
            .padding([7, 12])
            .style(style::danger_text(t))
            .on_press(Message::FormRemove)
            .into()
    } else {
        Space::new().into()
    };
    let footer = row![
        remove,
        Space::new().width(Fill),
        footer_button(t, "Cancel", false, Some(Message::CloseDialog)),
        footer_button(t, "Save", true, Some(Message::FormSave))
    ]
    .spacing(8)
    .align_y(Vertical::Center);
    dialog_frame(t, 440.0, Some(form.title), body.into(), footer.into())
}
pub(super) fn password<'a>(t: Tokens, form: &'a PasswordForm) -> Element<'a> {
    let mut body = column![label(form.account.as_str(), 13.0, t.ink)]
        .spacing(12)
        .padding(Padding {
            top: 14.0,
            right: 22.0,
            bottom: 18.0,
            left: 22.0,
        });
    if let Some(error) = &form.error {
        body = body.push(label(error.as_str(), 12.0, t.danger));
    }
    body = body
        .push(field_label(
            t,
            "Password",
            input(t, "", form.password.as_str(), |v| Message::PasswordInput(v.into()))
                .id(PASSWORD)
                .secure(true)
                .on_submit(Message::PasswordSubmit),
        ))
        .push(check_row(
            t,
            "Open full screen",
            form.fullscreen,
            Message::PasswordFullscreen,
        ))
        .push(check_row(t, "Remember this password", form.remember, Message::PasswordRemember))
        .push(label(
            if form.remember {
                "Use the Windows account password, not its PIN. It is kept in your system keyring, not in Win RDP's files."
            } else {
                "Use the Windows account password, not its PIN."
            },
            11.5,
            t.mute,
        ));
    let footer = row![
        Space::new().width(Fill),
        footer_button(t, "Cancel", false, Some(Message::CloseDialog)),
        footer_button(
            t,
            "Connect",
            true,
            (!form.password.is_empty()).then_some(Message::PasswordSubmit)
        )
    ]
    .spacing(8);
    dialog_frame(t, 440.0, Some(form.title.as_str()), body.into(), footer.into())
}
pub(super) fn confirm<'a>(t: Tokens, confirm: &'a Confirm) -> Element<'a> {
    let body = column![
        strong(confirm.title.as_str(), 15.0, t.ink),
        label(confirm.text.as_str(), 13.0, t.mute)
    ]
    .spacing(12)
    .padding(Padding {
        top: 18.0,
        right: 22.0,
        bottom: 18.0,
        left: 22.0,
    });
    let footer = row![
        Space::new().width(Fill),
        footer_button(t, confirm.yes, true, Some(Message::ConfirmYes)),
        footer_button(t, "Cancel", false, Some(Message::CloseDialog))
    ]
    .spacing(8);
    dialog_frame(t, 440.0, None, body.into(), footer.into())
}
