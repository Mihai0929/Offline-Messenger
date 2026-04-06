# Offline Messenger Beresteanu Mihail 2025-2026

A secureclient-server messaging application built entirely in **Rust**. This project allows real-time communication between users, seamless delivery of offline messages, and ensures privacy through connection-specific encryption.

Developed as the final project for the *Rust Programming* course at the Faculty of Computer Science, Alexandru Ioan Cuza University of Iași (UAIC).

## Key Features

* **Real-Time Messaging:** Instant exchange of messages between concurrently connected users.
* **Offline Message Queuing:** Messages sent to offline users are securely stored on the server and automatically delivered the moment they log back in.
* **Targeted Replies:** Users can reply to specific messages within the chat, maintaining clear context in conversations.
* **Persistent Conversation History:** The application maintains and provides a complete conversation history for each user individually.
* **End-to-End Encryption:** All communication is encrypted. The system generates unique, distinct encryption keys for every single client-server connection to ensure maximum privacy and data integrity.

## Running the project
Start the server
cargo run --bin server

Start a client
cargo run --bin gui

## Usage example
User A connects and sends a message to User B (who is currently offline).

The server encrypts the message and stores it in User B's pending queue.

User B boots up their client and connects to the server.

The server automatically pushes the pending messages to User B, decrypting them securely.

User B selects a specific message ID from the history and sends a targeted reply back to User A.
