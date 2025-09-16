use axum::{Router, response::Html, routing::get};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(hello_handler));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on http://0.0.0.0:3000");

    axum::serve(listener, app).await.unwrap();
}

async fn hello_handler() -> Html<&'static str> {
    println!("Client requested at endpoint '/', sending response.");
    Html("<h1>Hello WebSocket Server</h1>")
}
