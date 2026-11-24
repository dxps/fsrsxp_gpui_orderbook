//! Browser feed client. Everything runs on the main thread via
//! `wasm_bindgen_futures::spawn_local`, so passing snapshots back to GPUI is
//! as simple as writing a `Rc<RefCell<Shared>>` the renderer owns a handle
//! to. No synchronisation primitives needed.

use super::ConnStatus;
use fsrsxp_orderbook_proto::{MarketSnapshot, UiConfig};
use futures_util::StreamExt;
use gloo_net::http::Request;
use gloo_net::websocket::{futures::WebSocket, Message};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

struct Shared {
    latest: Option<MarketSnapshot>,
    status: ConnStatus,
}

pub struct FeedClient {
    shared: Rc<RefCell<Shared>>,
}

impl FeedClient {
    pub fn connect(config_url: String) -> Self {
        let shared = Rc::new(RefCell::new(Shared {
            latest: None,
            status: ConnStatus::Connecting,
        }));
        let shared2 = shared.clone();

        spawn_local(async move {
            let config: UiConfig = match Request::get(&config_url).send().await {
                Ok(resp) => match resp.json().await {
                    Ok(c) => c,
                    Err(err) => {
                        log::error!("feed: /ui-config decode: {err}");
                        shared2.borrow_mut().status = ConnStatus::Disconnected;
                        return;
                    }
                },
                Err(err) => {
                    log::error!("feed: /ui-config GET {config_url} failed: {err}");
                    shared2.borrow_mut().status = ConnStatus::Disconnected;
                    return;
                }
            };

            let ws = match WebSocket::open(&config.ws_url) {
                Ok(ws) => ws,
                Err(err) => {
                    log::error!("feed: ws connect {} failed: {err}", config.ws_url);
                    shared2.borrow_mut().status = ConnStatus::Disconnected;
                    return;
                }
            };
            shared2.borrow_mut().status = ConnStatus::Connected;

            let (_write, mut read) = ws.split();
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(t)) => {
                        if let Ok(snap) = serde_json::from_str::<MarketSnapshot>(&t) {
                            shared2.borrow_mut().latest = Some(snap);
                        }
                    }
                    Ok(Message::Bytes(_)) => continue,
                    Err(err) => {
                        log::error!("feed: ws read: {err}");
                        shared2.borrow_mut().status = ConnStatus::Disconnected;
                        return;
                    }
                }
            }
            shared2.borrow_mut().status = ConnStatus::Disconnected;
        });

        Self { shared }
    }

    pub fn try_latest(&mut self) -> Option<MarketSnapshot> {
        self.shared.borrow_mut().latest.take()
    }

    pub fn status(&self) -> ConnStatus {
        self.shared.borrow().status
    }
}
