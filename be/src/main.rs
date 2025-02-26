use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::any,
    Router,
};
use futures_util::{
    stream::{SplitSink, SplitStream, StreamExt},
    SinkExt,
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Clone)]
struct AppState {
    pub redis: Arc<redis::Client>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let addr_str = "0.0.0.0:3000";

    let app_state = AppState {
        redis: Arc::new(redis::Client::open("redis://0.0.0.0:6379").unwrap()),
    };

    let app = Router::new()
        .route("/ws", any(ws_handler))
        .with_state(app_state);
    let addr = tokio::net::TcpListener::bind(addr_str).await.unwrap();
    info!("Server running on ws://{}", addr_str);
    axum::serve(addr, app.into_make_service()).await.unwrap();
}

#[derive(Serialize, Deserialize)]
struct GameState {
    position: i64,
    left_pulls: i64,
    right_pulls: i64,
}

#[derive(Deserialize)]
struct Pull {
    action: String,
    direction: String,
}

async fn ws_handler(ws: WebSocketUpgrade, State(app_state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, app_state))
}

async fn handle_socket(socket: WebSocket, app_state: AppState) {
    info!("Player connected");

    let (sender, receiver) = socket.split();

    tokio::spawn(read(app_state.clone(), receiver));
    tokio::spawn(write(app_state, sender));
}

async fn read(app_state: AppState, mut receiver: SplitStream<WebSocket>) {
    let mut conn = app_state
        .redis
        .get_multiplexed_async_connection()
        .await
        .unwrap();

    // Initialize game state if not present
    let _: () = redis::pipe()
        .set_nx("position", 0)
        .set_nx("left_pulls", 0)
        .set_nx("right_pulls", 0)
        .query_async(&mut conn)
        .await
        .unwrap();

    // Handle incoming messages
    while let Some(Ok(msg)) = receiver.next().await {
        info!("Received: {:?}", msg);
        if let Message::Text(text) = msg {
            match serde_json::from_str::<Pull>(&text) {
                Ok(pull) if pull.action == "pull" => {
                    let (key, increment) = match pull.direction.as_str() {
                        "left" => ("left_pulls", -1),
                        "right" => ("right_pulls", 1),
                        _ => continue,
                    };

                    // Update state
                    let _: () = conn.incr(key, 1).await.unwrap();
                    let new_position: i64 = conn.incr("position", increment).await.unwrap();
                    let left_pulls: i64 = conn.get("left_pulls").await.unwrap();
                    let right_pulls: i64 = conn.get("right_pulls").await.unwrap();

                    let state = GameState {
                        position: new_position,
                        left_pulls,
                        right_pulls,
                    };

                    // Publish update
                    let _: () = conn
                        .publish("game_updates", serde_json::to_string(&state).unwrap())
                        .await
                        .unwrap();

                    // Check win and reset
                    // if new_position.abs() >= 10_000_000 {
                    //     let _: () = redis::pipe()
                    //         .set("position", 0)
                    //         .set("left_pulls", 0)
                    //         .set("right_pulls", 0)
                    //         .query_async(&mut conn)
                    //         .await
                    //         .unwrap();
                    //     let reset_state = GameState {
                    //         position: 0,
                    //         left_pulls: 0,
                    //         right_pulls: 0,
                    //     };
                    //     let _: () = pubsub
                    //         .publish("game_updates", serde_json::to_string(&reset_state).unwrap())
                    //         .await
                    //         .unwrap();
                    // }
                }
                _ => warn!("Invalid message: {}", text),
            }
        }
    }
}

async fn write(app_state: AppState, mut sender: SplitSink<WebSocket, Message>) {
    let mut pubsub = app_state.redis.get_async_pubsub().await.unwrap();
    pubsub.subscribe("game_updates").await.unwrap();

    let mut conn = app_state
        .redis
        .get_multiplexed_async_connection()
        .await
        .unwrap();

    // Send initial state
    let state = get_game_state(&mut conn).await;
    if sender
        .send(Message::Text(serde_json::to_string(&state).unwrap().into()))
        .await
        .is_err()
    {
        return;
    }

    // Handle pubsub messages
    while let Some(msg) = pubsub.on_message().next().await {
        let payload: String = msg.get_payload().unwrap();
        if sender.send(Message::Text(payload.into())).await.is_err() {
            return;
        }
    }
}

async fn get_game_state(conn: &mut redis::aio::MultiplexedConnection) -> GameState {
    let position: i64 = conn.get("position").await.unwrap();
    let left_pulls: i64 = conn.get("left_pulls").await.unwrap();
    let right_pulls: i64 = conn.get("right_pulls").await.unwrap();
    GameState {
        position,
        left_pulls,
        right_pulls,
    }
}
