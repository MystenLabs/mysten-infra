mod example;

use anyhow::anyhow;
use async_trait::async_trait;
use futures::future;
use tokio::signal;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot::{
    channel as oneshotChannel, Receiver as oneshotReceiver, Sender as oneshotSender,
};
use tokio::time::{timeout, Duration};

type IrrecoverableError = anyhow::Error;
type TokioJoinHandle = tokio::task::JoinHandle<()>;

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
        rx_cancelllation: oneshotReceiver<()>,
    ) -> tokio::task::JoinHandle<()>; // Not the task is "silent" (returns nothing) and does its work through side effects

    // The API for cleanup after the task has encountered an irrecoverable error
    async fn handle_irrecoverable(
        &self,
        irrecoverable: IrrecoverableError,
    ) -> Result<(), anyhow::Error>;
}

pub struct Component<M: Manageable> {
    shutdown_signal: oneshotSender<()>,
    panic_signal: Receiver<IrrecoverableError>,
    join_handle: Option<TokioJoinHandle>,
    manageable: M,
}

impl<M: Manageable + Send + Clone> Component<M> {
    pub fn new(launcher: M) -> Self {
        // initialize start_fn
        // optionally, initialize the shutdown signal, the panic signal,
        // initialize join_handle to None
        let (_, tr_panic) = channel(10);
        let (tx_shutdown, _) = oneshotChannel();
        Component{
            shutdown_signal: tx_shutdown,
            panic_signal: tr_panic,
            join_handle: None,
            manageable: launcher
        }
    }

    // calls th launcher & stores the join handle
    async fn spawn(mut self) -> Result<(), anyhow::Error> {
        let (tx_panic, tr_panic) = channel(10);
        let (tx_shutdown, tr_shutdown) = oneshotChannel();

        // this assumes the start_fn is populated
        let wrapped_handle = self.manageable.start(tx_panic, tr_shutdown).await;

        self.panic_signal = tr_panic;
        self.shutdown_signal = tx_shutdown;
        self.join_handle = Some(wrapped_handle);

        // we're ourselves a component
        self.run().await
    }

    // Run supervision of the child task
    async fn run(mut self) -> Result<(), anyhow::Error> {
        // TODO : how does this conflict with join ? Do we still want join?
        // I don't think we need to join, because we never expect these tasks to complete
        // exception is on application shutdown, which theoretically should never happen.
        //
        // select statement that listens for the following cases:
        //
        // panic signal incoming => log, terminate and restart
        // completion of the task => we're done! return

        if let Some(mut handle) = self.join_handle {
            loop {
                tokio::select! {
                    Some(message) = self.panic_signal.recv() => {

                        println!("panic message received: {:?}", message.to_string());
                        self.manageable.handle_irrecoverable(message).await;
                        self.terminate().await?;

                        // restart, continuous passing style
                        let next_component = Component::new(self.manageable.clone());
                        self = next_component;
                        self.spawn();
                    }

                   // Poll the JoinHandle<O>
                   result = &mut handle => {
                        // this is normal termination of the task returning result: O
                        // HAPPY PATH :D
                        break
                    }


                }
            }
            Ok(())
        } else {
            // this has been called before initialization
            return Err(anyhow!(
                "Component has not yet been launched, cannot run supervision."
            ));
        }
    }

    // Do we want to block the component on the customer's task?
    pub async fn join(self) -> Result<(), anyhow::Error> {
        if let Some(handle) = self.join_handle {
            handle.await;
        } else {
            // this has been called before initialization
            return Err(anyhow!(
                "Component has not yet been launched, cannot join."
            ));
        }
        Ok(())
    }

    const GRACE_TIMEOUT: Duration = Duration::from_secs(2);

    async fn terminate(self) -> Result<(), anyhow::Error> {
        self.shutdown_signal.send(()).unwrap();

        if let Some(mut handle) = self.join_handle {
            // poll (rather than join) the handle for completion for 2s (poll is non-consuming)
            // and then abort
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Self::GRACE_TIMEOUT) =>{
                        println!("did not receive completion within {:?} s", Self::GRACE_TIMEOUT);
                        handle.abort();
                        break
                    }
                    result = &mut handle => {
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

        Ok(())
    }
}
