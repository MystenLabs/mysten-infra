

use anyhow::anyhow;
use async_trait::async_trait;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot::{
    channel as oneshotChannel, Receiver as oneshotReceiver, Sender as oneshotSender,
};
use tokio::time::Duration;

pub type IrrecoverableError = anyhow::Error;
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
    cancellation_signal: Option<oneshotSender<()>>,
    irrecoverable_signal: Receiver<IrrecoverableError>,
    join_handle: Option<JoinHandle>,
    manageable: M,
}

impl<M: Manageable + Send> Supervisor<M> {
    pub fn new(component: M) -> Self {
        // initialize start_fn
        // optionally, initialize the shutdown signal, the panic signal,
        // initialize join_handle to None
        let (_, tr_irrecoverable) = channel(10);
        let (tx_cancellation, _) = oneshotChannel();
        Supervisor {
            cancellation_signal: Some(tx_cancellation),
            irrecoverable_signal: tr_irrecoverable,
            join_handle: None,
            manageable: component,
        }
    }

    // calls th launcher & stores the join handle
    pub async fn spawn(mut self) -> Result<(), anyhow::Error> {
        let (tx_irrecoverable, tr_irrecoverable) = channel(10);
        let (tx_cancellation, tr_cancellation) = oneshotChannel();

        // this assumes the start_fn is populated
        let wrapped_handle = self
            .manageable
            .start(tx_irrecoverable, tr_cancellation)
            .await;

        self.irrecoverable_signal = tr_irrecoverable;
        self.cancellation_signal = Some(tx_cancellation);
        self.join_handle = Some(wrapped_handle);

        self.run().await
    }

    // Run supervision of the child task
    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        // select statement that listens for the following cases:
        //
        // Irrecoverable signal incoming => log, terminate and restart
        // completion of the task => we're done! return
        loop {
            tokio::select! {
                Some(message) = self.irrecoverable_signal.recv() => {
                    self.manageable.handle_irrecoverable(message).await?;
                    self.terminate().await?;
                    println!("I have terminated the component");
                    // restart
                    let (panic_sender, panic_receiver) = channel(10);
                    let (cancel_sender, cancel_receiver) = oneshotChannel();

                    // call the launcher
                    let wrapped_handle: tokio::task::JoinHandle<()> =  self.manageable.start(panic_sender, cancel_receiver).await;
                    println!("I have spawned the next component");
                    // reset the supervision handles & channel end points
                    self.irrecoverable_signal = panic_receiver;
                    self.cancellation_signal = Some(cancel_sender);
                    self.join_handle = Some(wrapped_handle);
                },

               // Poll the JoinHandle<O>
               _result =  self.join_handle.as_mut().unwrap(), if self.join_handle.is_some() => {
                    // this is normal termination of the task returning result: O
                    // HAPPY PATH :D
                    return Ok(())
                }
            }
        }
    }

    const GRACE_TIMEOUT: Duration = Duration::from_secs(2);

    async fn terminate(&mut self) -> Result<(), anyhow::Error> {

        match self.cancellation_signal.take() {
            Some(c) => {
                match c.send(()) {
                    Ok(_) => {},
                    Err(e) => return Err(anyhow!("error found {:?}", e))
                };
            },
            None => return Err(anyhow!("The cancellation signal was consumed"))
        };
        //todo: we are not seeing the terminating accept loop print .send(()).AWAIT!


        // poll (rather than join) the handle for completion for 2s (poll is non-consuming)
        // and then abort
        if let Some(ref mut handle) = self.join_handle {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Self::GRACE_TIMEOUT) => {
                        println!("did not receive completion within {:?} s", Self::GRACE_TIMEOUT);
                        self.join_handle.as_ref().unwrap().abort();
                        break;
                    }
                    _result = handle => {
                        break;
                    }
                }
            }
            Ok(())
        } else {
            // this has been called before initialization
            return Err(anyhow!(
                "Component has not yet been launched, cannot terminate."
            ));
        }
    }
}
