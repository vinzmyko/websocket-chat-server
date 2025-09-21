use std::sync::Arc;

use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::get,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use names::Generator;
use tokio::{net::TcpListener, sync::mpsc};
use uuid::Uuid;

pub struct ConnectedClient {
    pub user_name: String,
    pub sender: mpsc::UnboundedSender<Message>,
}

#[tokio::main]
async fn main() {
    // create the sharded hashmap<string, ConnectedClient> for async
    let clients = Arc::new(DashMap::new());

    // handles how ip address endpoints map to our code
    let app = Router::new()
        .route("/ws", get(handle_websocket))
        .with_state(clients);

    // tracks traffic from specific ip address
    let ip_addr = "0.0.0.0:3000";
    let listener = TcpListener::bind(ip_addr).await.unwrap();
    println!("Server running on http://{}", ip_addr);

    // connects the server and the listener
    axum::serve(listener, app).await.unwrap();
}

async fn handle_websocket(
    ws: WebSocketUpgrade,
    State(clients): State<Arc<DashMap<Uuid, ConnectedClient>>>,
) -> impl IntoResponse {
    println!("WebSocket upgrade requested");

    let mut generator = Generator::default();
    let user_name = generator.next().unwrap();

    // upgrades http connection and forwards connection to bespoke websocket connection
    ws.on_upgrade(move |socket| handle_connection(socket, clients, user_name))
}

async fn handle_connection(
    ws: WebSocket,
    clients: Arc<DashMap<Uuid, ConnectedClient>>,
    user_name: String,
) {
    println!("WebSocket connection established");
    let client_id = Uuid::new_v4();

    let (mut sender, mut receiver) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel();

    clients.insert(
        client_id,
        ConnectedClient {
            user_name: user_name.clone(),
            sender: tx,
        },
    );

    let welcome_msg = format!("Welcome to chat, {}! You are now connected.", user_name);
    let welcome_message = Message::Text(welcome_msg);
    if let Err(e) = sender.send(welcome_message).await {
        println!("Cannot send to {} Error: {}", user_name, e);
    };

    // create background worker task for tracking traffic to the channel
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    while let Some(msg) = receiver.next().await {
        if let Ok(Message::Text(text)) = msg {
            let m = format!("{}: {}", user_name, text);

            let serialised_data = rmp_serde::to_vec(&m).unwrap();

            // now we can loop over the clients
            for entry in clients.iter() {
                let id = entry.key();
                let client = entry.value();

                // create the guard clause
                if *id == client_id {
                    continue;
                }

                // need to clone it because we are sending the same data to multiple clients
                let message = Message::Binary(serialised_data.clone());
                // we need to send over the message to the channel
                if let Err(e) = client.sender.send(message) {
                    println!("Cannot send to {} Error: {}", client.user_name, e);
                }
            }
        }
    }

    clients.remove(&client_id);
}
