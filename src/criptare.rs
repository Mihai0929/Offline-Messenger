use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{AeadMut, OsRng},
};

use x25519_dalek::{EphemeralSecret, PublicKey};

use sha2::{Digest, Sha256};

//Am nevoie de o structura sa retin cheia secreta pana realizam Handshake-ul
pub struct RememberSecret {
    secret_key: EphemeralSecret,
    pub public_key: PublicKey,
}

impl Default for RememberSecret {
    fn default() -> Self {
        Self::new()
    }
}

impl RememberSecret {
    pub fn new() -> Self {
        //generez o cheie secreta random
        let secret_key = EphemeralSecret::random_from_rng(OsRng);
        //derivez cheia publica in functie de cea privata
        let public_key = PublicKey::from(&secret_key);

        Self {
            secret_key,
            public_key,
        }
    }

    pub fn derive_key(self, public_key_bytes: Vec<u8>) -> [u8; 32] {
        let key_expected: [u8; 32] = public_key_bytes
            .try_into()
            .expect("Cheia publica trebuie sa aiba 32 bytes!");

        //Convertim array-ul intr-o cheie publica
        let aux_key = PublicKey::from(key_expected);

        let shared_secret = self.secret_key.diffie_hellman(&aux_key);

        let mut hash_func = Sha256::new();
        hash_func.update(shared_secret.as_bytes());

        hash_func.finalize().into()
    }
}

pub struct ChannelSecure {
    nonce_cnt: u64,
    cifru: Aes256Gcm,
}

impl ChannelSecure {
    pub fn new(curr_key: [u8; 32]) -> Self {
        Self {
            nonce_cnt: 0,
            cifru: Aes256Gcm::new(&curr_key.into()),
        }
    }

    pub fn encrypt(&mut self, info: &[u8]) -> Result<Vec<u8>, aes_gcm::Error> {
        //Cream counter-ul
        let mut nonce_bytes = [0u8; 12];

        nonce_bytes[..8].copy_from_slice(&self.nonce_cnt.to_be_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);

        //Criptam continutul
        let res = self.cifru.encrypt(nonce, info)?;

        //Formam pachetul pe care vrem sa-l trimitem(Nonce-ul si cifrul)
        let mut package = Vec::new();
        package.extend_from_slice(&self.nonce_cnt.to_be_bytes());
        package.extend(res);

        self.nonce_cnt += 1;
        Ok(package)
    }

    pub fn decrypt(&mut self, crypted_content: &[u8]) -> Result<Vec<u8>, aes_gcm::Error> {
        //Impartim continutul, mai apoi
        let (nonce_bytes, cipher_text) = crypted_content.split_at(8);

        let mut initial_nonce = [0u8; 12];
        initial_nonce[..8].copy_from_slice(nonce_bytes);
        let nonce = Nonce::from_slice(&initial_nonce);

        self.cifru.decrypt(nonce, cipher_text)
    }
}
