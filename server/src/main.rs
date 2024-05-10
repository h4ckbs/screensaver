use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use futures::{Future, FutureExt, StreamExt};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;

use http::status::StatusCode;

use warp::ws::{Message, WebSocket};
use warp::{Filter, Rejection, Reply};

type Result<T> = std::result::Result<T, Rejection>;
type Clients = Arc<Mutex<HashMap<String, Client>>>;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct FrontendEvent {
    amount: u64,
    comment: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Event {
    comment: Option<Vec<String>>,
    amount: u64,
    payment_hash: String,
    payment_request: String,
    webhook_data: String,
    lnurlp: String,
    body: String,
}

pub struct Client {
    pub sender: Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>>,
}

pub async fn client_connection(ws: WebSocket, clients: Clients) {
    let (client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();

    let client_rcv = UnboundedReceiverStream::new(client_rcv);
    tokio::task::spawn(client_rcv.forward(client_ws_sender).map(|result| {
        if let Err(e) = result {
            eprintln!("error sending websocket msg: {}", e);
        }
    }));

    let client = Client {
        sender: Some(client_sender),
    };
    clients
        .lock()
        .await
        .insert(uuid::Uuid::new_v4().to_string(), client);
}

pub async fn ws_handler(ws: warp::ws::Ws, clients: Clients) -> Result<impl Reply> {
    println!("ws");
    Ok(ws.on_upgrade(move |socket| client_connection(socket, clients)))
}

pub fn health_handler() -> impl Future<Output = Result<impl Reply>> {
    println!("health");
    futures::future::ready(Ok(StatusCode::OK))
}

pub async fn publish_handler(body: Event, clients: Clients) -> Result<impl Reply> {
    println!("a");
    dbg!(&body);
    clients.lock().await.iter().for_each(|(_, client)| {
        dbg!("client");
        if let Some(sender) = &client.sender {
            dbg!("sender");
            let e = FrontendEvent {
                amount: body.amount,
                comment: body.comment.clone().unwrap_or(vec!["".to_string()])[0].clone(),
            };
            let _ = sender.send(Ok(Message::text(serde_json::to_string(&e).unwrap())));
        }
    });

    Ok(StatusCode::OK)
}

#[tokio::main]
async fn main() {
    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));

    let health_route = warp::path!("health").and_then(health_handler);

    let publish = warp::path!("publish")
        .and(warp::body::json())
        .and(with_clients(clients.clone()))
        .and_then(publish_handler);

    let ws_route = warp::path("ws")
        .and(warp::ws())
        // .and(warp::path::param())
        .and(with_clients(clients.clone()))
        .and_then(ws_handler);

    let routes = health_route
        .or(ws_route)
        .or(publish)
        .with(warp::cors().allow_any_origin());

    warp::serve(routes).run(([127, 0, 0, 1], 2347)).await;
}

fn with_clients(clients: Clients) -> impl Filter<Extract = (Clients,), Error = Infallible> + Clone {
    warp::any().map(move || clients.clone())
}
