use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, State,
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
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tracing::{info, warn};

#[derive(Clone)]
struct AppState {
    redis: Arc<redis::Client>,
    active_users: Arc<AtomicUsize>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let addr_str = "0.0.0.0:3000";
    let app_state = AppState {
        redis: Arc::new(
            redis::Client::open("redis://0.0.0.0:6379").expect("Failed to connect to Redis"),
        ),
        active_users: Arc::new(AtomicUsize::new(0)),
    };

    let app = Router::new()
        .route("/ws", any(ws_handler))
        .with_state(app_state);
    let addr = tokio::net::TcpListener::bind(addr_str)
        .await
        .expect("Failed to bind to port");
    info!("Server running on ws://{}", addr_str);
    axum::serve(
        addr,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("Failed to start server");
}

#[derive(Serialize, Deserialize)]
struct GameState {
    position: i64,
    left_pulls: i64,
    right_pulls: i64,
    active_users: usize,
}

#[derive(Deserialize)]
struct Pull {
    action: String,
    direction: String,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(app_state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    info!("New player connected from {}", addr.ip());
    ws.on_upgrade(move |socket| handle_socket(socket, app_state, addr.ip().to_string()))
}

async fn handle_socket(socket: WebSocket, app_state: AppState, ip: String) {
    info!("Player connected from {}", ip);
    app_state.active_users.fetch_add(1, Ordering::SeqCst);

    let (sender, receiver) = socket.split();

    let read_task = tokio::spawn(read(app_state.clone(), receiver, ip));
    let write_task = tokio::spawn(write(app_state.clone(), sender));

    tokio::select! {
        _ = read_task => info!("Read task ended"),
        _ = write_task => info!("Write task ended"),
    }
    app_state.active_users.fetch_sub(1, Ordering::SeqCst);
}

async fn read(app_state: AppState, mut receiver: SplitStream<WebSocket>, ip: String) {
    let mut conn = app_state
        .redis
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect to Redis");

    let _: () = redis::pipe()
        .set_nx("position", 0)
        .set_nx("left_pulls", 0)
        .set_nx("right_pulls", 0)
        .query_async(&mut conn)
        .await
        .expect("Failed to initialize game state");

    while let Some(Ok(msg)) = receiver.next().await {
        info!("Received: {:?}", msg);
        if let Message::Text(text) = msg {
            match serde_json::from_str::<Pull>(&text) {
                Ok(pull) if pull.action == "pull" => {
                    // Rate limiting: 1 pull per second per IP
                    let key = format!("rate_limit:{}", ip);
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("Time went backwards")
                        .as_secs();
                    let count: i64 = conn.zcount(&key, now - 1, now).await.unwrap_or(0);
                    if count >= 10 {
                        warn!("Rate limit exceeded for IP: {}", ip);
                        continue;
                    }
                    conn.zadd::<_, _, _, String>(&key, now, now)
                        .await
                        .expect("Failed to rate limit");
                    conn.zrangebyscore::<_, _, _, Vec<String>>(&key, 0, now - 2)
                        .await
                        .expect("Failed to clean up rate limit"); // Clean old entries
                    conn.expire::<_, String>(&key, 10)
                        .await
                        .expect("Failed to set expiry"); // Expire key after 10s

                    let (key, increment) = match pull.direction.as_str() {
                        "left" => ("left_pulls", -1),
                        "right" => ("right_pulls", 1),
                        _ => continue,
                    };

                    let _: () = conn
                        .incr(key, 1)
                        .await
                        .expect("Failed to increment pull count");
                    let new_position: i64 = conn
                        .incr("position", increment)
                        .await
                        .expect("Failed to increment position");
                    let left_pulls: i64 = conn
                        .get("left_pulls")
                        .await
                        .expect("Failed to get left pulls");
                    let right_pulls: i64 = conn
                        .get("right_pulls")
                        .await
                        .expect("Failed to get right pulls");

                    let state = GameState {
                        position: new_position,
                        left_pulls,
                        right_pulls,
                        active_users: app_state.active_users.load(Ordering::SeqCst),
                    };

                    let _: () = conn
                        .publish(
                            "game_updates",
                            serde_json::to_string(&state).expect("Failed to serialize state"),
                        )
                        .await
                        .expect("Failed to publish game state");
                }
                _ => warn!("Invalid message: {}", text),
            }
        }
    }
}

// (write and get_game_state unchanged, just update app_state.clone() in write call)
async fn write(app_state: AppState, mut sender: SplitSink<WebSocket, Message>) {
    let mut pubsub = app_state
        .redis
        .get_async_pubsub()
        .await
        .expect("Failed to get pubsub");
    pubsub
        .subscribe("game_updates")
        .await
        .expect("Failed to subscribe to game_updates");

    let mut conn = app_state
        .redis
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect to Redis");
    let state = get_game_state(&mut conn, &app_state).await;
    if sender
        .send(Message::Text(
            serde_json::to_string(&state)
                .expect("Failed to serialize state")
                .into(),
        ))
        .await
        .is_err()
    {
        return;
    }

    while let Some(msg) = pubsub.on_message().next().await {
        let payload: String = msg.get_payload().expect("Failed to get payload");
        if sender.send(Message::Text(payload.into())).await.is_err() {
            return;
        }
    }
}

async fn get_game_state(
    conn: &mut redis::aio::MultiplexedConnection,
    app_state: &AppState,
) -> GameState {
    let position: i64 = conn.get("position").await.expect("Failed to get position");
    let left_pulls: i64 = conn
        .get("left_pulls")
        .await
        .expect("Failed to get left pulls");
    let right_pulls: i64 = conn
        .get("right_pulls")
        .await
        .expect("Failed to get right pulls");
    GameState {
        position,
        left_pulls,
        right_pulls,
        active_users: app_state.active_users.load(Ordering::SeqCst),
    }
}
