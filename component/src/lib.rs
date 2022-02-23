use tokio::{sync::mpsc::{channel, Receiver, }};
use std::future::Future;

type IrrecoverableError = std::io::Error;

pub struct Component {
    complete: Receiver<()>,
    handle: tokio::task::JoinHandle<Result<(), IrrecoverableError>>,
}

pub struct ComponentManager {
   // message_receivers: Vec<Receiver<IrrecoverableError>>,
    handles: Vec<Component>,
}

impl ComponentManager {
    fn new() -> ComponentManager {
        // let mut message_receivers = Vec::new();
        let mut handles = Vec::new();
        ComponentManager{handles}
    }

    fn spawn<F>(&mut self, future:F) -> &Component //  input param: function that takes a sender as a param
        where F: Future<Output = Result<(), IrrecoverableError>> + Send + 'static,
    {
        let (sender, mut receiver) = channel(10);

        let wrapped_handle = tokio::spawn(future(sender));
        let mut component = Component{complete: receiver, handle: wrapped_handle};
        self.handles.push(component);
        &component
    }

    // async fn run(&self) {
    //     tokio::select! {
    //         Some(result) = self.message_receivers.recv() => {
    //             // log and restart
    //         }
    //         _ = self.futures.iter_mut() => {
    //             //restart
    //         }
    //
    //     }
        // let mut select_all = select_all(&self.message_receivers);
        // loop {
        //     tokio::select! {
        //         msg = select_all.next() => {
        //             // log and restart
        //         }
        //         _ = self.futures.select_next_some() => {
        //             // restart
        //         }
        //     }
        // }
    // }
}



#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
