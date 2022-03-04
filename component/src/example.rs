use crate::{Manageable, IrrecoverableError, spawn, run_supervision};

use futures::future;
use futures::stream::FuturesUnordered;
use tokio::{sync::mpsc::{channel, Sender, Receiver}};
use tokio::sync::oneshot::{channel as oneshotChannel, Sender as oneshotSender, Receiver as oneshotReceiver};

#[derive(Copy, Clone)]
pub struct ComponentTypeA {}

impl Manageable for ComponentTypeA {
    fn start(&self, panic_signal: Sender<IrrecoverableError>, shutdown_signal: oneshotReceiver<()>) -> future::BoxFuture<'static, ()> {
        match something_that_must_happen() {
            Ok(..) => {}
            Err(error) => {
                panic_signal.send(error).await;
            }
        };

        loop {
            tokio::select! {
                _ = shutdown_signal => {
                    return Ok(());
                }
                // usual case goes here, ie TCP listener, etc.
            }
        }
    }
}

pub fn something_that_must_happen() -> Result<(), std::io::Error> {
    return Ok(())
}

#[tokio::main]
async fn main() {
    let mut handles = Vec::new();
    let a = ComponentTypeA {};
    let componentManager = spawn(a);
    handles.push(run_supervision(componentManager).await);

    future::join_all(handles).await;
}