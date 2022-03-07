use crate::{run_supervision, spawn, IrrecoverableError, Manageable};

use async_trait::async_trait;
use futures::future;
use futures::stream::FuturesUnordered;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot::{
    channel as oneshotChannel, Receiver as oneshotReceiver, Sender as oneshotSender,
};

#[derive(Copy, Clone)]
pub struct ComponentTypeA {}

#[async_trait]
impl Manageable for ComponentTypeA {
    async fn start(
        &self,
        panic_signal: Sender<IrrecoverableError>,
        shutdown_signal: oneshotReceiver<()>,
    ) -> () {
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
    return Ok(());
}

#[tokio::main]
async fn main() {
    let mut handles = Vec::new();
    let a = ComponentTypeA {};
    let componentManager = spawn(a);
    handles.push(run_supervision(componentManager).await);

    future::join_all(handles).await;
}
