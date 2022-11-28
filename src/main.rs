use std::borrow::{Borrow, BorrowMut};
use std::fs;
use awc::{BoxedSocket, Client, ClientResponse};
use awc::error::WsClientError;
use awc::ws::{Codec, Frame, Message};
use futures::{SinkExt, StreamExt};
use actix_codec::Framed;
use async_std::io;
use bytestring::ByteString;
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};
use toml;

#[actix::main]
async fn main() {
    // TODO: only allow connections to servers that are online
    // TODO: terminate on shutdown

    match connect().await {
        Ok((_, mut connection)) => {
            let stdin = io::stdin();
            let mut sent_command = false;

            loop {
                tokio::select! {
                     Some(msg) = read_next_message(connection.borrow_mut()) => {
                        if !sent_command {
                            handle_message(msg, connection.borrow_mut()).await;
                        }
                        else {
                            sent_command = false;
                        }
                     }
                     Some(line) = read_next_line(stdin.borrow()) => {
                        send_command(line, connection.borrow_mut()).await;
                        sent_command = true;
                    }
                }
            }
        }
        Err(error) => {
            println!("Failed to connect to websocket: {:?}", error);
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    token: String,
    server: String,
}

impl Config {
    pub fn load() -> Result<Config, io::Error> {
        let data = fs::read_to_string("./config.toml")?;
        let mut config: Config = toml::from_str(&data)?;
        config.server = config.server.replace("#", "");

        Ok(config)
    }
}


async fn connect() -> Result<(ClientResponse, Framed<BoxedSocket, Codec>), WsClientError> {
    let config = Config::load().expect("Failed to load config. Make sure your config is valid");

    let client = Client::builder()
        .max_http_version(awc::http::Version::HTTP_11)
        .finish();

    return client
        .ws(format!("wss://api.exaroton.com/v1/servers/{}/websocket", config.server))
        .set_header("User-Agent", "JulianVennen/exaroton-console-cli/0.1.0 julian@vennen.me")
        .set_header("Authorization", format!("Bearer {}", config.token))
        .connect().await;
}

async fn read_next_message(connection: &mut Framed<BoxedSocket, Codec>) -> Option<Value> {
    let response = connection.next().await;
    if let Some(Ok(Frame::Text(bytes))) = response {
        let string = String::from_utf8(Vec::from(bytes.as_ref())).unwrap();
        let message: Value = serde_json::from_str(string.as_str()).unwrap();
        return Some(message);
    }
    None
}

async fn handle_message(message: Value, connection: &mut Framed<BoxedSocket, Codec>) {
    if let Value::Object(message) = message {
        if let Some(Value::String(r#type)) = message.get("type") {
            match r#type.as_str() {
                "ready" => {
                    subscribe_to_console_stream(connection).await
                }
                "started" => {
                    if let Some(Value::String(stream)) = message.get("stream") {
                        if stream.as_str() == "console" {
                            println!("Subscribed to console stream.");
                        }
                    }
                }
                "line" => {
                    if let Some(Value::String(data)) = message.get("data") {
                        println!("{}", data);
                    }
                }
                "keep-alive" => {
                    //println!("Received keep-alive.");
                }
                _ => {
                    //println!("Received new message of type {}: {:?}", r#type, message);
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct ConsoleStreamMessage {
    stream: String,
    r#type: String,
    data: Value,
}

impl ConsoleStreamMessage {
    pub fn new<S: Into<String>>(r#type: S, data: Value) -> ConsoleStreamMessage {
        ConsoleStreamMessage {
            stream: String::from("console"),
            r#type: r#type.into(),
            data,
        }
    }
}

async fn subscribe_to_console_stream(connection: &mut Framed<BoxedSocket, Codec>) {
    let message = ConsoleStreamMessage::new("start", json!({
        "tail": 100
    }));
    let json = serde_json::to_string(&message).unwrap();
    let bytes = ByteString::from(json);
    connection.send(Message::Text(bytes)).await
        .expect("Failed to subscribe to console stream!");
}

async fn read_next_line(stdin: &io::Stdin) -> Option<String> {
    let mut line = String::new();
    match stdin.read_line(&mut line).await {
        Ok(_) => Some(line.replace('\n', "")),
        Err(..) => None
    }
}

async fn send_command(command: String, connection: &mut Framed<BoxedSocket, Codec>) {
    let message = ConsoleStreamMessage::new("command", Value::String(command));
    let json = serde_json::to_string(&message).unwrap();
    let bytes = ByteString::from(json);
    connection.send(Message::Text(bytes)).await
        .expect("Failed to subscribe to console stream!");
}
