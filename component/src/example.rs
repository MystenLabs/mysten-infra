use crate::{IrrecoverableError, Manageable};

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
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancelllation: oneshotReceiver<()>,
    ) -> tokio::task::JoinHandle<()> {
        let handle = tokio::spawn(something_that_must_happen(
            tx_irrecoverable,
            rx_cancelllation,
        ));

        handle
    }

    async fn handle_irrecoverable(
        &self,
        irrecoverable: IrrecoverableError,
    ) -> Result<(), anyhow::Error> {
        todo!()
    }
}

pub async fn something_that_must_happen(
    tx_irrecoverable: Sender<anyhow::Error>,
    rx_cancelllation: oneshotReceiver<()>,
) {
    // using cancellation handle and irrecoverable sender goes here
    return;
}

#[tokio::main]
async fn main() {
    let mut handles = Vec::new();
    let a = ComponentTypeA {};
    let componentManager = spawn(a);
    handles.push(run_supervision(componentManager).await);

    future::join_all(handles).await;
}
