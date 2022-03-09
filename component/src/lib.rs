mod example;

use anyhow::anyhow;
use async_trait::async_trait;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot::{
    channel as oneshotChannel, Receiver as oneshotReceiver, Sender as oneshotSender,
};
use tokio::time::Duration;

type IrrecoverableError = anyhow::Error;
type JoinHandle = tokio::task::JoinHandle<()>;

// The User defines a task launcher, that takes:
// - an irrecoverable error sender, on which IrrecoverableError is signaled in the task
// - a cancellation handle, which will be listened to in the task
// it then equips the task with those, launches it and returns the JoinHandle
#[async_trait]
pub trait Manageable {
    // The API for the task launcher
    async fn start(
        &self,
        tx_irrecoverable: Sender<anyhow::Error>,
        rx_cancellation: oneshotReceiver<()>,
    ) -> tokio::task::JoinHandle<()>; // Not the task is "silent" (returns nothing) and does its work through side effects

    // The API for cleanup after the task has encountered an irrecoverable error
    async fn handle_irrecoverable(
        &self,
        irrecoverable: IrrecoverableError,
    ) -> Result<(), anyhow::Error>;
}

pub struct Supervisor<M: Manageable> {
    cancellation_signal: oneshotSender<()>,
    irrecoverable_signal: Receiver<IrrecoverableError>,
    join_handle: Option<JoinHandle>,
    manageable: M,
}

impl<M: Manageable + Send + Clone> Supervisor<M> {
    pub fn new(component: M) -> Self {
        // initialize start_fn
        // optionally, initialize the shutdown signal, the panic signal,
        // initialize join_handle to None
        let (_, tr_irrecoverable) = channel(10);
        let (tx_cancellation, _) = oneshotChannel();
        Supervisor {
            cancellation_signal: tx_cancellation,
            irrecoverable_signal: tr_irrecoverable,
            join_handle: None,
            manageable: component
        }
    }

    // calls th launcher & stores the join handle
    async fn spawn(mut self) -> Result<(), anyhow::Error> {
        let (tx_irrecoverable, tr_irrecoverable) = channel(10);
        let (tx_cancellation, tr_cancellation) = oneshotChannel();

        // this assumes the start_fn is populated
        let wrapped_handle = self.manageable.start(tx_irrecoverable, tr_cancellation).await;

        self.irrecoverable_signal = tr_irrecoverable;
        self.cancellation_signal = tx_cancellation;
        self.join_handle = Some(wrapped_handle);

        self.run().await
    }

    // Run supervision of the child task
    async fn run(mut self) -> Result<(), anyhow::Error> {
        // select statement that listens for the following cases:
        //
        // Irrecoverable signal incoming => log, terminate and restart
        // completion of the task => we're done! return

        if let Some(mut handle) = self.join_handle {
            loop {
                tokio::select! {
                    Some(message) = self.irrecoverable_signal.recv() => {

                        self.manageable.handle_irrecoverable(message).await;
                        self.terminate().await?;

                        // restart
                        let (panic_sender, panic_receiver) = channel(10);
                        let (cancel_sender, cancel_receiver) = oneshotChannel();

                        let wrapped_handle: tokio::task::JoinHandle<()> =  self.manageable.start(panic_sender, cancel_receiver).await;

                        self.irrecoverable_signal = panic_receiver;
                        self.cancellation_signal = cancel_sender;
                        self.join_handle = Some(wrapped_handle);

                        // // restart, continuous passing style
                        // let next_component = Supervisor::new(self.manageable.clone());
                        // self = next_component;
                        // self.spawn().await;
                    }

                   // Poll the JoinHandle<O>
                   result = mut handle => {
                        // this is normal termination of the task returning result: O
                        // HAPPY PATH :D
                        return Ok(())
                    }
                }
            }
        } else {
            // this has been called before initialization
            return Err(anyhow!(
                "Component has not yet been launched, cannot run supervision."
            ));
        }
    }

    const GRACE_TIMEOUT: Duration = Duration::from_secs(2);

    async fn terminate(mut self) -> Result<(), anyhow::Error> {
        self.cancellation_signal.send(()).unwrap();


        // poll (rather than join) the handle for completion for 2s (poll is non-consuming)
        // and then abort
        if let Some(mut handle) = self.join_handle {
            loop {
                tokio::select! {
                _ = tokio::time::sleep(Self::GRACE_TIMEOUT) =>{
                    println!("did not receive completion within {:?} s", Self::GRACE_TIMEOUT);
                    self.join_handle.as_ref().unwrap().abort();
                    break
                }
                result = mut handle => {
                    break
                }
            }
            }
        } else {
            // this has been called before initialization
            return Err(anyhow!(
                "Component has not yet been launched, cannot terminate."
            ));
        }
    }
}
