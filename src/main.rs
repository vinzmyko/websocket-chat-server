use std::{net::SocketAddr, process, sync::Arc};

use axum::{
    Router,
    extract::{
        ConnectInfo, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::get,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use names::Generator;
use tokio::{net::TcpListener, sync::mpsc};
use tracing::{debug, error, info};
use uuid::Uuid;

pub struct ConnectedClient {
    pub user_name: String,
    pub sender: mpsc::UnboundedSender<Message>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    // create the sharded hashmap<string, ConnectedClient> for async
    let clients = Arc::new(DashMap::new());

    // handles how ip address endpoints map to our code
    let app = Router::new()
        .route("/ws", get(handle_websocket))
        .with_state(clients);

    // tracks traffic from specific ip address
    let ip_addr = "0.0.0.0:3000";
    let listener = match TcpListener::bind(ip_addr).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!(
                "[FATAL] Couldn't bind to hard coded ip_addr '{}' error: {}",
                ip_addr, e
            );
            process::exit(1);
        }
    };
    println!("Server running on http://{}", ip_addr);

    // connects the server and the listener
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

async fn handle_websocket(
    ws: WebSocketUpgrade,
    State(clients): State<Arc<DashMap<Uuid, ConnectedClient>>>,
    ConnectInfo(ip_addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    println!("WebSocket upgrade requested");

    let mut generator = Generator::default();
    let user_name = generator.next().unwrap();

    // upgrades http connection and forwards connection to bespoke websocket connection
    ws.on_upgrade(move |socket| handle_connection(socket, clients, user_name, ip_addr))
}

async fn handle_connection(
    ws: WebSocket,
    clients: Arc<DashMap<Uuid, ConnectedClient>>,
    user_name: String,
    client_ip: SocketAddr,
) {
    let client_id = Uuid::new_v4();
    info!(client_id = %client_id, user_name = %user_name, client_ip_address = %client_ip, "Websocket connection established");

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
            info!(sender_id = %client_id, sender_name = %user_name, message = %text, "Received message from client");
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
                match client.sender.send(message) {
                    Ok(_) => {
                        debug!(sending_to_id = %id, sender_id = %client_id, "Sending message to {}", id);
                    }
                    Err(e) => {
                        error!(sending_to_id = %id, sender_id = %client_id, "Cannot send to {} Error: {}", client.user_name, e);
                    }
                }
            }
        }
    }

    info!(client_id = %client_id, user_name = %user_name, client_ip_address = %client_ip, "Client {} has disconnected", client_id);
    clients.remove(&client_id);
}
