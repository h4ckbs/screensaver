use std::collections::HashMap;
use std::sync::Arc;
use std::convert::Infallible;

use futures::{StreamExt, Future, FutureExt};
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;

use http::status::StatusCode;

use warp::ws::{Message, WebSocket};
use warp::{Filter, Reply, Rejection};

type Result<T> = std::result::Result<T, Rejection>;
type Clients = Arc<Mutex<HashMap<String, Client>>>;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Event {
    description: String,
    amount: u64,
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
        sender: Some(client_sender)
    };
    clients.lock().await.insert(uuid::Uuid::new_v4().to_string(), client);
}

pub async fn ws_handler(ws: warp::ws::Ws, clients: Clients) -> Result<impl Reply> {
  Ok(ws.on_upgrade(move |socket| client_connection(socket, clients)))
}

pub fn health_handler() -> impl Future<Output = Result<impl Reply>> {
    futures::future::ready(Ok(StatusCode::OK))
}

pub async fn publish_handler(body: Event, clients: Clients) -> Result<impl Reply> {
    dbg!(&body);
  clients
    .lock()
    .await
    .iter()
    .for_each(|(_, client)| {
        dbg!("client");
      if let Some(sender) = &client.sender {
        dbg!("sender");
        let _ = sender.send(Ok(Message::text(serde_json::to_string(&body).unwrap())));
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
