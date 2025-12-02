use aes_gcm::aead::OsRng;

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
