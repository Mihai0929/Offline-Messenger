use eframe::egui;
use project::ClientChat;
use project::protocol::{Message, MessageHistoryInfo};
use chrono::{DateTime, Local, Utc};


const SERVER_ADDRESS: &str = "127.0.0.1:2024";

fn format_time(secs: i64) -> String {
    if let Some(utc_time) = DateTime::<Utc>::from_timestamp(secs, 0){
        let local_time: DateTime<Local> = utc_time.with_timezone(&Local);
        local_time.format("%d/%m/%Y %H:%M:%S").to_string()
    }
    else{
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
    message_history: Vec<String>,
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
            message_history: Vec::new(),
        };

        // încercăm să ne conectăm imediat, fără buton
        this.try_to_connect();
        this
    }

    fn try_to_connect(&mut self) {
        self.connect_error = None;

        match ClientChat::connect(SERVER_ADDRESS) {
            Ok(client) => {
                self.client = Some(client);
                self.message_history
                    .push(format!("Conectat la {}", SERVER_ADDRESS));
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
            self.message_history
                .push("Nu esti conectat la server".to_owned());
            return;
        };

        self.message_history.clear();

        let msg = Message::HistoryInfo {
            user: person.to_string(),
        };

        if let Err(e) = client.send_message(msg) {
            self.message_history
                .push(format!("Eroare cerere istoric: {}", e));
        }
    }

    
    fn send_current_message(&mut self) {
        let person = self.curr_person.trim();
        let text = self.message_input.trim();

        if person.is_empty() || text.is_empty() {
            return;
        }

        let Some(client) = self.client.as_mut() else {
            self.message_history
                .push("Nu esti conectat la server".to_owned());
            return;
        };

        let msg = Message::Text {
            to: person.to_string(),
            content: text.to_string(),
            reply_id: None,
        };

        match client.send_message(msg) {
            Ok(()) => {
                //formatam timpul
                let time_secs = chrono::Local::now().timestamp();
                let time_str = format_time(time_secs);

                let sender = if self.username.trim().is_empty(){
                    "curr_user".to_string()
                }
                else{
                    self.username.clone()
                };

                //afisam mesajul local
                self.message_history
                    .push(format!("[{time_str}] {sender}: {text}"));
                self.message_input.clear();
            }
            Err(e) => {
                self.message_history
                    .push(format!("Eroare la trimitere: {}", e));
            }
        }
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
                } => {
                    let time_str = format_time(time);
                    self.message_history
                        .push(format!("#{id} [{time_str}] {from}: {content}"));
                }

                // istoricul pentru conversatia curenta
                Message::HistoryData { content } => {
                    self.message_history.clear();
                    for MessageHistoryInfo {
                        message_id,
                        sender,
                        content,
                        time,
                        delivered,
                    } in content
                    {
                        let time_str = format_time(time);
                        let ok = if delivered { "" } else { " (Mesaj nelivrat)" };
                        self.message_history
                            .push(format!("#{message_id} [{time_str}] {sender}: {content}{ok}",));
                    }
                }

                // fallback pentru alte tipuri de mesaje
                other => {
                    self.message_history.push(format!("Server: {:?}", other));
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

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.message_history {
                        ui.label(line);
                    }
                });
        });
    }
}
