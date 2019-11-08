use gtk::prelude::*;
use gtk::{FileChooserAction, FileChooserDialog, FileChooserExt, Inhibit, ResponseType};
use relm::{init, Component, Widget};
use relm_derive::{widget, Msg};

use self::Msg::*;

mod encrypt;

pub struct Model {
    encrypt: Option<Component<encrypt::Win>>,
}

#[derive(Msg)]
pub enum Msg {
    OpenEncrypt,
    Quit,
}

#[widget]
impl Widget for Win {
    fn model() -> Model {
        Model { encrypt: None }
    }

    fn update(&mut self, event: Msg) {
        match event {
            OpenEncrypt => self.open_encrypt(),
            Quit => gtk::main_quit(),
        }
    }

    view! {
        #[name="window"]
        gtk::Window {
            title: "Girage",
            delete_event(_, _) => (Quit, Inhibit(false)),

            gtk::HeaderBar {
                gtk::Button {
                    label: "Encrypt",
                    clicked => OpenEncrypt,
                },
            },
        }
    }
}

impl Win {
    fn open_encrypt(&mut self) {
        let dialog = FileChooserDialog::new(
            Some("Open a file"),
            Some(&self.window),
            FileChooserAction::Open,
        );
        dialog.set_select_multiple(true);
        dialog.add_button("Cancel", ResponseType::Cancel);
        dialog.add_button("Open", ResponseType::Accept);
        let result = dialog.run();
        if result == ResponseType::Accept {
            self.model.encrypt =
                Some(init::<encrypt::Win>(dialog.get_filenames()).expect("encrypt window"));
        }
        dialog.destroy();
    }
}

fn main() {
    Win::run(()).unwrap();
}
