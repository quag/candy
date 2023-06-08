//! Unlike other language server features, hints are not generated on-demand
//! with the usual request-response model. Instead, a hints server runs in the
//! background all the time. That way, the hints can progressively get better.
//! For example, when opening a long file, the hints may appear from top to
//! bottom as more code is evaluated. Then, the individual functions could get
//! fuzzed with ever-more-complex inputs, resulting in some error cases to be
//! displayed over time.
//!
//! While doing all that, we can pause regularly between executing instructions
//! so that we don't occupy a single CPU at 100 %.

use self::{hint::Hint, hints_finder::HintsFinder};
use crate::database::Database;
use candy_frontend::{
    module::{Module, MutableModuleProviderOwner, PackagesPath},
    rich_ir::ToRichIr,
};
use lsp_types::{notification::Notification, Diagnostic, Url};
use candy_frontend::module::{Module, MutableModuleProviderOwner, PackagesPath};
use itertools::Itertools;
use lsp_types::{notification::Notification, Position, Url};
use rand::{seq::IteratorRandom, thread_rng};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::{fmt, time::Duration, vec};
use tokio::{
    sync::mpsc::{error::TryRecvError, Receiver, Sender},
    time::sleep,
};
use tracing::debug;

pub mod hint;
mod hints_finder;
mod utils;

pub enum Event {
    UpdateModule(Module, Vec<u8>),
    CloseModule(Module),
    Shutdown,
}

#[derive(Serialize, Deserialize)]
pub struct HintsNotification {
    pub uri: Url,
    pub hints: Vec<Hint>,
}
impl Notification for HintsNotification {
    const METHOD: &'static str = "candy/textDocument/publishHints";

    type Params = Self;
}

#[tokio::main(worker_threads = 1)]
#[allow(unused_must_use)]
pub async fn run_server(
    packages_path: PackagesPath,
    status_sender: Sender<String>,
    mut incoming_events: Receiver<Event>,
    outgoing_hints: Sender<(Module, Vec<Hint>)>,
    outgoing_diagnostics: Sender<(Module, Vec<Diagnostic>)>,
) {
    let mut db = Database::new_with_file_system_module_provider(packages_path);
    let mut hints_finders: FxHashMap<Module, HintsFinder> = FxHashMap::default();
    let mut outgoing_hints = OutgoingCache::new(outgoing_hints);
    let mut outgoing_diagnostics = OutgoingCache::new(outgoing_diagnostics);

    'server_loop: loop {
        sleep(Duration::from_millis(100)).await;

        loop {
            let event = match incoming_events.try_recv() {
                Ok(event) => event,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break 'server_loop,
            };
            match event {
                Event::UpdateModule(module, content) => {
                    db.did_change_module(&module, content);
                    outgoing_hints.send(module.clone(), vec![]).await;
                    hints_finders
                        .entry(module.clone())
                        .and_modify(|it| it.module_changed())
                        .or_insert_with(|| HintsFinder::for_module(module.clone()));
                }
                Event::CloseModule(module) => {
                    db.did_close_module(&module);
                    hints_finders.remove(&module);
                }
                Event::Shutdown => {
                    incoming_events.close();
                }
            }
        }

        let Some(module) = hints_finders.keys().choose(&mut thread_rng()).cloned() else { 
            status_sender
                .send(format!("🍭"))
                .await
                .unwrap();
            continue;
        };
        let hints_finder = hints_finders.get_mut(&module).unwrap();

        let status_sender_clone = status_sender.clone();
        let set_status = move |status: Option<String>| {
            let status_sender_clone = status_sender_clone.clone();
            async move {
                let status_string = match status {
                    Some(status) => format!("🍭 {status}"),
                    None => "🍭".to_string(),
                };
                status_sender_clone.send(status_string).await.unwrap()
            }
        };
        hints_finder.run(&db, &set_status).await;

        let (mut hints, diagnostics) = hints_finder.hints(&db, &module);
        hints.sort_by_key(|hint| hint.position);

        outgoing_diagnostics.send(module.clone(), diagnostics).await;
        outgoing_hints.send(module, hints).await;
    }
}

struct OutgoingCache<T> {
    sender: Sender<(Module, T)>,
    last_sent: FxHashMap<Module, T>,
}
impl<T: Clone + fmt::Debug + Eq> OutgoingCache<T> {
    fn new(sender: Sender<(Module, T)>) -> Self {
        Self {
            sender,
            last_sent: FxHashMap::default(),
        }
    }

    async fn send(&mut self, module: Module, value: T) {
        if self.last_sent.get(&module) != Some(&value) {
            debug!("Reporting for {}: {value:?}", module);
            self.last_sent.insert(module.clone(), value.clone());
            self.sender.send((module, value)).await.unwrap();
        }
    }
}
