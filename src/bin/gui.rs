use chrono::{DateTime, Local, Utc};
use eframe::egui;
use project::ClientChat;
use project::protocol::{Message, MessageHistoryInfo};

const SERVER_ADDRESS: &str = "127.0.0.1:2024";

fn format_time(secs: i64) -> String {
    if let Some(utc_time) = DateTime::<Utc>::from_timestamp(secs, 0) {
        let local_time: DateTime<Local> = utc_time.with_timezone(&Local);
        local_time.format("%d/%m/%Y %H:%M:%S").to_string()
    } else {
        "Invalid time".to_string()
    }
}

pub fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        "Offline Messenger",
        native_options,
        Box::new(|cc| Ok(Box::new(Messenger::new(cc)))),
    )
}

struct ChatMessage {
    id: Option<u64>,
    sender: String,
    content: String,
    time: i64,
    reply_to: Option<u64>,
}

struct Messenger {
    // conexiune
    client: Option<ClientChat>,
    connect_error: Option<String>,

    // login
    username: String,
    password: String,
    logged_status: String,

    // conversatie curenta
    curr_person: String,
    message_input: String,
    messages: Vec<ChatMessage>,
    reply_to: Option<u64>,
}

impl Messenger {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut this = Self {
            client: None,
            connect_error: None,

            username: String::new(),
            password: String::new(),
            logged_status: String::new(),

            curr_person: String::new(),
            message_input: String::new(),

            messages: Vec::new(),
            reply_to: None,
        };

        this.try_to_connect();
        this
    }

    fn try_to_connect(&mut self) {
        self.connect_error = None;

        match ClientChat::connect(SERVER_ADDRESS) {
            Ok(client) => {
                self.client = Some(client);
                self.messages.push(ChatMessage {
                    id: None,
                    sender: "System".to_string(),
                    content: format!("Conectat la adresa {}", SERVER_ADDRESS),
                    time: Local::now().timestamp(),
                    reply_to: None,
                });
            }
            Err(e) => {
                self.connect_error = Some(format!("Eroare la conectare: {}", e));
            }
        }
    }

    fn login(&mut self) {
        let Some(client) = self.client.as_mut() else {
            self.logged_status = "Nu esti conectat la server".to_owned();
            return;
        };

        if self.username.trim().is_empty() || self.password.is_empty() {
            self.logged_status = "Completeaza username si parola".to_owned();
            return;
        }

        let msg = Message::Login {
            username: self.username.clone(),
            password: self.password.clone(),
        };

        match client.send_message(msg) {
            Ok(()) => {
                self.logged_status = "Cerere login trimisa".to_owned();
            }
            Err(e) => {
                self.logged_status = format!("Eroare trimitere login: {e}");
            }
        }
    }

    ///preluam istoricul conversatiei
    fn open_conversation(&mut self) {
        let person = self.curr_person.trim();
        if person.is_empty() {
            return;
        }

        let Some(client) = self.client.as_mut() else {
            self.messages.push(ChatMessage {
                id: None,
                sender: "System".to_string(),
                content: "Nu esti conectat la server!".to_owned(),
                time: Local::now().timestamp(),
                reply_to: None,
            });
            return;
        };

        self.messages.clear();
        self.reply_to = None;

        let msg = Message::HistoryInfo {
            user: person.to_string(),
        };

        if let Err(e) = client.send_message(msg) {
            self.messages.push(ChatMessage {
                id: None,
                sender: "System".to_string(),
                content: format!("Eroare la preluarea istoricului! {}", e),
                time: Local::now().timestamp(),
                reply_to: None,
            })
        }
    }

    fn send_current_message(&mut self) {
        let person = self.curr_person.trim().to_string();
        let text = self.message_input.trim().to_string();

        if person.is_empty() || text.is_empty() {
            return;
        }

        let Some(client) = self.client.as_mut() else {
            self.messages.push(ChatMessage {
                id: None,
                sender: "System".to_string(),
                content: "Nu esti conectat la server".to_owned(),
                time: Local::now().timestamp(),
                reply_to: None,
            });
            return;
        };

        let reply_id = self.reply_to;

        let msg = Message::Text {
            to: person.clone(),
            content: text.clone(),
            reply_id,
        };

        match client.send_message(msg) {
            Ok(()) => {
                //formatam timpul
                let time_secs = Local::now().timestamp();

                let sender = if self.username.trim().is_empty() {
                    "curr_user".to_string()
                } else {
                    self.username.clone()
                };

                //afisam mesajul local
                self.messages.push(ChatMessage {
                    id: None,
                    sender,
                    content: text,
                    time: time_secs,
                    reply_to: reply_id,
                });

                //dupa ce dam reply nu mai tinem flag-ul pentru reply
                self.reply_to = None;
                self.message_input.clear();
            }
            Err(e) => {
                self.messages.push(ChatMessage {
                    id: None,
                    sender: "System".to_string(),
                    content: format!("Eroare la trimitere: {}", e),
                    time: Local::now().timestamp(),
                    reply_to: None,
                });
            }
        }
    }

    fn reply_message(&mut self, id: u64) {
        self.reply_to = Some(id);
    }

    //preluam toate mesajele de la server
    fn messages_incoming(&mut self) {
        let Some(client) = self.client.as_ref() else {
            return;
        };

        while let Some(msg) = client.try_recv() {
            match msg {
                // mesaj de livrat
                Message::ToSend {
                    id,
                    from,
                    content,
                    time,
                    reply_id,
                } => {
                    self.messages.push(ChatMessage {
                        id: Some(id),
                        sender: from,
                        content,
                        time,
                        reply_to: reply_id,
                    });
                }

                // istoricul pentru conversatia curenta
                Message::HistoryData { content } => {
                    self.messages.clear();
                    for MessageHistoryInfo {
                        message_id,
                        sender,
                        content,
                        time,
                        delivered: _,
                        reply_id,
                    } in content
                    {
                        self.messages.push(ChatMessage {
                            id: Some(message_id),
                            sender,
                            content,
                            time,
                            reply_to: reply_id,
                        });
                    }
                }

                // fallback pentru alte tipuri de mesaje
                other => {
                    self.messages.push(ChatMessage {
                        id: None,
                        sender: "Server".to_string(),
                        content: format!("{:?}", other),
                        time: Local::now().timestamp(),
                        reply_to: None,
                    });
                }
            }
        }
    }
}

impl eframe::App for Messenger {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        use std::time::Duration;

        //procesam iar aplicatia ca sa primim mesajele de la server
        ctx.request_repaint_after(Duration::from_millis(100));
        self.messages_incoming();

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.client.is_some() {
                    ui.label("Conectat la server");
                } else {
                    ui.label("Nu esti conectat");
                }

                if let Some(err) = &self.connect_error {
                    ui.colored_label(egui::Color32::RED, err);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Utilizator:");
                ui.text_edit_singleline(&mut self.username);

                ui.label("Parola:");
                ui.add(egui::TextEdit::singleline(&mut self.password).password(true));

                if ui.button("Login").clicked() {
                    self.login();
                }
            });
        });

        egui::TopBottomPanel::bottom("input_panel").show(ctx, |ui| {
            if let Some(id) = self.reply_to
                && let Some(msg) = self.messages.iter().find(|m| m.id == Some(id))
            {
                let mut preview = msg.content.clone();
                if preview.len() > 40 {
                    preview.truncate(40);
                    preview.push_str("...");
                }
                ui.horizontal(|ui| {
                    ui.label(format!("Raspunzi lui {}: {}", msg.sender, preview));
                    if ui.button("Anuleaza").clicked() {
                        self.reply_to = None;
                    }
                });
            }

            ui.separator();
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.message_input)
                        .hint_text("Scrie un mesaj..."),
                );

                let send_clicked = ui.button("Trimite").clicked();
                let enter_pressed =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                if send_clicked || enter_pressed {
                    self.send_current_message();
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Către:");
                ui.text_edit_singleline(&mut self.curr_person);

                if ui.button("Deschide chat").clicked() {
                    self.open_conversation();
                }
            });

            ui.separator();

            let mut clicked_id: Option<u64> = None;

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for msg in &self.messages {
                        let time_string = format_time(msg.time);
                        let text = format!("[{time_string}] {}: {}", msg.sender, msg.content);

                        ui.group(|ui| {
                            if let Some(rid) = msg.reply_to
                                && let Some(orig) = self.messages.iter().find(|m| m.id == Some(rid))
                            {
                                let mut preview = orig.content.clone();
                                if preview.len() > 40 {
                                    preview.truncate(40);
                                    preview.push_str("...");
                                }
                                ui.label(format!("Raspuns lui {}: {}", orig.sender, preview));
                            }

                            let response = ui.label(text);

                            //in caz ca se da click pe mesaj se da reply
                            if response.clicked()
                                && let Some(id) = msg.id
                            {
                                clicked_id = Some(id);
                            }
                        });
                    }
                });

            if let Some(id) = clicked_id {
                self.reply_message(id);
            }
        });
    }
}
