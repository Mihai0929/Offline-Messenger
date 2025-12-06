use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHistoryInfo {
    pub message_id: u64,
    pub sender: String,
    pub content: String,
    pub time: i64,
    pub delivered: bool,
}

//Fac un enunm corespunzator protocolului propus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    //Realizarea handshake-ului

    //Clientul trimite cheia publica catre server
    ClientKey {
        public_key: Vec<u8>,
    },

    //Serverul raspunde cu cheia publica
    ServerKey {
        public_key: Vec<u8>,
    },

    //Autentificare
    Login {
        username: String,
        password: String,
    },

    //Clientul trimite mesajul catre server(in caz ca raspunde la un mesaj retin ID-ul )
    Text {
        to: String,
        content: String,
        reply_id: Option<u64>,
    },

    //Mesajele sunt trimise de catre server(retin timpul mesajelor trimise in caz ca sunt offline pentru a fi distribuite in ordinea corecta)
    ToSend {
        id: u64,
        from: String,
        content: String,
        time: i64,
    },

    //Istoricul pe care il vreau de la o persoana respectiva
    HistoryInfo {
        user: String,
    },

    //Retin mesajele trimise pentru a afisa istoricul
    HistoryData {
        content: Vec<MessageHistoryInfo>,
    },
}
