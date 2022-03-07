mod example;

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
        &mut self,
        irrecoverable: IrrecoverableError,
    ) -> Result<(), anyhow::Error>;
}

pub struct Component<M: Manageable> {
    shutdown_signal: oneshotSender<()>,
    panic_signal: Receiver<IrrecoverableError>,
    join_handle: Option<TokioJoinHandle>,
    start_fn: M,
}

impl<M: Manageable + Send + Clone> Component<M> {
    pub fn new<Mg: Manageable>(launcher: Mg) -> Self {
        // initialize start_fn
        // optionally, initialize the shutdown signal, the panic signal,
        // initialize join_handle to None
        todo!()
    }

    // actually calls th launcher & stores the join handle
    async fn spawn(mut self, starter: M) -> Result<(), anyhow::Error> {
        let (panic_sender, panic_receiver) = channel(10);
        let (cancel_sender, cancel_receiver) = oneshotChannel();

        // this assumes the start_fn is populated
        let wrapped_handle = starter.start(panic_sender, cancel_receiver).await;

        self.panic_signal = panic_receiver;
        self.shutdown_signal = cancel_sender;
        self.join_handle = Some(wrapped_handle);

        // we're ourselves a component
        self.run().await
    }

    // Run supervision of the child task
    async fn run(mut self) -> Result<(), anyhow::Error> {
        // TODO : how does this conflict with join ? Do we still want join?
        // TODO : terminate is public .. does that make sense ?
        // TODO : there's a lot in common between
        //  -> new () which sets up the launcher we want to dedicate ourselves to
        // -> spawn() which sets up the supervision, and launches the task
        // -> restart () which cleans up (optionally as defined by the user), re-sets up supervision, and launches the task

        // select statement that listens for the following cases:
        //
        // panic signal incoming => log, terminate and restart
        // completion of the task => we're done! return
        //
        // ctrl + c signal => send shutdown signal
        //

        loop {
            tokio::select! {
                Some(message) = self.panic_signal.recv() => {

                    println!("panic message received: {:?}", message.to_string());
                    self.start_fn.handle_irrecoverable(message).await;
                    // self.terminate().await?;

                    // restart
                    // This is eerily similar to spawn
                    let (panic_sender, panic_receiver) = channel(10);
                    let (cancel_sender, cancel_receiver) = oneshotChannel();

                    let wrapped_handle: tokio::task::JoinHandle<()> =  self.start_fn.start(panic_sender, cancel_receiver).await;

                    self.panic_signal = panic_receiver;
                    self.shutdown_signal = cancel_sender;
                    self.join_handle = Some(wrapped_handle);

                }

                // Poll the JoinHandle<O>
               // Some(result) = self.join_handle => {
                    // this is normal termination of the task returning result: O
                    // HAPPY PATH :D

                //    break
                //}

                _ = signal::ctrl_c() => {
                    self.terminate().await?;
                    break
                }
            }
        }
        Ok(())
    }

    // Implementation notes:
    // how do we periodically poll
    // - the completion of the child task (join handle)?
    // - the reception of an irrecoverable signal (self.panic_signal)?
    // => then, how do we restart after cleanup?

    // Do we want to block the component on the customer's task?
    pub async fn join(self) -> Result<(), std::io::Error> {
        if let Some(handle) = self.join_handle {
            handle.await;
        } else {
            // this has been called before initialization
            todo!()
        }
        Ok(())
    }

    pub async fn terminate(self) -> Result<(), std::io::Error> {
        self.shutdown_signal.send(()).unwrap();

        if let Some(handle) = self.join_handle {
            // TODO : *poll* (rather than join) the handle for completion for 2s (pool is non-consuming)
            // and then abort

            //if let Err(_) = timeout(Duration::from_secs(2), handle).await {
            handle.abort();
            //}
        } else {
            // this has been called before initialization!!!
            todo!()
        }

        Ok(())
    }
}
