use age::{keys::RecipientKey, Encryptor};
use gtk::prelude::*;
use gtk::{FileChooserAction, FileChooserDialog, Inhibit, Orientation::Vertical, ResponseType};
use relm::{Channel, EventStream, Relm, Widget};
use relm_derive::{widget, Msg};
use secrecy::SecretString;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::thread;

use self::EncryptMsg::*;

const MINIMUM_PASSPHRASE_LENGTH: u16 = 6;

pub struct Model {
    filenames: Vec<PathBuf>,
    recipients: Vec<(String, RecipientKey)>,
    stream: EventStream<EncryptMsg>,
    encrypt_channel: Option<Channel<io::Result<()>>>,
}

#[derive(Msg)]
pub enum EncryptMsg {
    Change,
    StartEncrypt,
    FinishEncrypt,
    Close,
}

#[allow(clippy::cognitive_complexity)]
#[widget]
impl Widget for Win {
    fn model(relm: &Relm<Self>, filenames: Vec<PathBuf>) -> Model {
        assert!(!filenames.is_empty());
        Model {
            filenames,
            recipients: vec![],
            stream: relm.stream().clone(),
            encrypt_channel: None,
        }
    }

    fn init_view(&mut self) {
        self.to_passphrase.join_group(Some(&self.to_recipients));

        if self.model.filenames.len() == 1 {
            // Encrypting a single file. The file list never changes, so we only need to
            // set this up once.
            self.encrypt_separately.set_visible(false);
        }

        self.update_ui();
    }

    fn update(&mut self, event: EncryptMsg) {
        match event {
            Change => self.update_ui(),
            StartEncrypt => self.model.encrypt_channel = self.select_encrypt_output(),
            FinishEncrypt => {
                self.window.close();
            }
            Close => self.window.close(),
        }
    }

    view! {
        #[name="window"]
        gtk::Window {
            title: "Encrypt Files - Girage",
            delete_event(_, _) => (Close, Inhibit(false)),

            gtk::Box {
                orientation: Vertical,

                gtk::Frame {
                    label: Some("Encrypt"),
                    margin_top: 10,
                    margin_bottom: 10,
                    margin_start: 10,
                    margin_end: 10,

                    gtk::Box {
                        orientation: Vertical,

                        gtk::Box {
                            #[name="to_recipients"]
                            gtk::RadioButton {
                                label: "Encrypt to recipients:",
                                clicked => Change,
                            },

                            #[name="add_recipient"]
                            gtk::Entry {
                            },
                        },

                        #[name="recipients"]
                        gtk::TreeView {
                        },

                        gtk::Box {
                            #[name="to_passphrase"]
                            gtk::RadioButton {
                                label: "Encrypt with passphrase:",
                                clicked => Change,
                            },

                            #[name="passphrase"]
                            gtk::Entry {
                                visibility: false,
                                changed => Change,
                            },
                        },
                    },
                },

                gtk::Frame {
                    label: Some("Output"),
                    margin_bottom: 10,
                    margin_start: 10,
                    margin_end: 10,

                    gtk::Box {
                        orientation: Vertical,

                        #[name="armor"]
                        gtk::CheckButton {
                            label: "Generate ASCII-armored output.",
                        },

                        #[name="encrypt_separately"]
                        gtk::CheckButton {
                            label: "Encrypt each file separately.",
                        },
                    },
                },

                gtk::ActionBar {
                    #[name="encrypt"]
                    gtk::Button {
                        label: "Encrypt",
                        clicked => StartEncrypt,
                    },

                    gtk::Button {
                        label: "Cancel",
                        clicked => Close,
                    },
                }
            },
        }
    }
}

impl Win {
    fn update_ui(&mut self) {
        let to_recipients = self.to_recipients.get_active();
        self.add_recipient.set_sensitive(to_recipients);
        self.passphrase.set_sensitive(!to_recipients);

        self.encrypt.set_sensitive(if to_recipients {
            !self.model.recipients.is_empty()
        } else {
            self.passphrase.get_text_length() >= MINIMUM_PASSPHRASE_LENGTH
        });
    }

    fn select_encrypt_output(&self) -> Option<Channel<io::Result<()>>> {
        let input_paths = self.model.filenames.clone();
        let multiple_files = input_paths.len() > 1;
        let encrypt_separately = self.encrypt_separately.get_active();
        let armor = self.armor.get_active();

        let dialog = if multiple_files {
            if encrypt_separately {
                FileChooserDialog::new(
                    Some("Select a folder"),
                    Some(&self.window),
                    FileChooserAction::CreateFolder,
                )
            } else {
                let dialog = FileChooserDialog::new(
                    Some("Save encrypted archive"),
                    Some(&self.window),
                    FileChooserAction::Save,
                );
                dialog.set_do_overwrite_confirmation(true);
                dialog.set_filename(
                    input_paths[0]
                        .parent()
                        .expect("has parent")
                        .join("archive.tar.age"),
                );
                dialog
            }
        } else {
            let dialog = FileChooserDialog::new(
                Some("Save encrypted file"),
                Some(&self.window),
                FileChooserAction::Save,
            );
            dialog.set_do_overwrite_confirmation(true);
            dialog.set_filename(age_extension(&self.model.filenames[0]));
            dialog
        };
        dialog.add_button("Cancel", ResponseType::Cancel);
        dialog.add_button("Save", ResponseType::Accept);
        let result = dialog.run();
        let output_path = dialog.get_filename();
        dialog.destroy();

        if result == ResponseType::Accept {
            let encryptor = if self.to_recipients.get_active() {
                Encryptor::Keys(
                    self.model
                        .recipients
                        .iter()
                        .map(|(_, k)| k)
                        .cloned()
                        .collect(),
                )
            } else {
                Encryptor::Passphrase(SecretString::new(
                    self.passphrase
                        .get_text()
                        .expect("get_text failed")
                        .to_string(),
                ))
            };

            let spinner = gtk::Spinner::new();
            let dialog = gtk::Dialog::new_with_buttons(
                Some("Encrypting..."),
                Some(&self.window),
                gtk::DialogFlags::MODAL,
                &[],
            );
            dialog
                .get_content_area()
                .pack_start(&spinner, true, true, 20);

            // TODO: Figure out why spinner doesn't show up.
            spinner.start();
            spinner.show();
            dialog.show();

            let stream = self.model.stream.clone();
            let (channel, sender) = Channel::new(move |res| {
                if let Err(e) = res {
                    // TODO: Do something useful with the error.
                    eprintln!("{}", e);
                }
                dialog.destroy();
                stream.emit(FinishEncrypt);
            });

            thread::spawn(move || {
                sender
                    .send(encrypt(
                        encryptor,
                        input_paths,
                        output_path.expect("have filename"),
                        armor,
                        encrypt_separately,
                    ))
                    .expect("send message");
            });

            Some(channel)
        } else {
            None
        }
    }
}

fn age_extension(filename: &PathBuf) -> PathBuf {
    let mut output_filename = filename.clone();
    if let Some(ext) = filename.extension() {
        output_filename.set_extension(format!(
            "{}.age",
            ext.to_str().expect("extension is valid UTF-8")
        ));
    } else {
        output_filename.set_extension("age");
    };
    output_filename
}

fn encrypt(
    encryptor: Encryptor,
    input_paths: Vec<PathBuf>,
    output_path: PathBuf,
    armor: bool,
    encrypt_separately: bool,
) -> io::Result<()> {
    if input_paths.len() > 1 {
        if encrypt_separately {
            for input_path in &input_paths {
                encrypt_single_file(
                    &encryptor,
                    input_path,
                    age_extension(
                        &output_path.join(input_path.file_name().expect("valid filename")),
                    ),
                    armor,
                )?;
            }
            Ok(())
        } else {
            encrypt_archive(&encryptor, input_paths, output_path, armor)
        }
    } else {
        encrypt_single_file(&encryptor, &input_paths[0], output_path, armor)
    }
}

fn encrypt_single_file(
    encryptor: &Encryptor,
    input_path: &PathBuf,
    output_path: PathBuf,
    armor: bool,
) -> io::Result<()> {
    let mut input = File::open(input_path)?;
    let mut output = encryptor.wrap_output(File::create(output_path)?, armor)?;
    io::copy(&mut input, &mut output)?;
    output.finish()?;
    Ok(())
}

fn encrypt_archive(
    encryptor: &Encryptor,
    input_paths: Vec<PathBuf>,
    output_path: PathBuf,
    armor: bool,
) -> io::Result<()> {
    let mut archive = tar::Builder::new(encryptor.wrap_output(File::create(output_path)?, armor)?);
    for input_path in &input_paths {
        let mut input = File::open(input_path)?;
        archive.append_file(
            input_path.file_name().expect("filename is valid UTF-8"),
            &mut input,
        )?;
    }
    let output = archive.into_inner()?;
    output.finish()?;
    Ok(())
}
