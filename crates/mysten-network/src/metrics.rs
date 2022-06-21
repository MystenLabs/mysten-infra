use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tonic::codegen::http::{Request, Response};
use tower::{Layer, Service};

/// A function called on every performed request (completed)
/// where various information are passed useful for metrics
/// purposes.
pub type MetricsRequestCallback = fn(path: String, latency: Duration, status: u16);

#[derive(Clone)]
pub struct RequestMetricsLayer {
    metrics_callback: MetricsRequestCallback,
}

impl RequestMetricsLayer {
    pub fn new(metrics_callback: MetricsRequestCallback) -> Self {
        RequestMetricsLayer { metrics_callback }
    }
}

impl<S> Layer<S> for RequestMetricsLayer {
    type Service = RequestMetrics<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestMetrics::new(inner, self.metrics_callback)
    }
}

#[derive(Clone)]
pub struct RequestMetrics<S> {
    inner: S,
    metrics_callback: MetricsRequestCallback,
}

impl<S> RequestMetrics<S> {
    pub fn new(inner: S, metrics_callback: MetricsRequestCallback) -> Self {
        RequestMetrics {
            inner,
            metrics_callback,
        }
    }
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for RequestMetrics<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let start = Instant::now();
        let path = req.uri().path().to_owned();

        ResponseFuture {
            response_future: self.inner.call(req),
            path,
            start,
            metrics_callback: self.metrics_callback,
        }
    }
}

#[pin_project]
pub struct ResponseFuture<F> {
    #[pin]
    response_future: F,
    #[pin]
    path: String,
    #[pin]
    start: Instant,
    #[pin]
    metrics_callback: MetricsRequestCallback,
}

impl<F, ResBody, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<Response<ResBody>, E>>,
{
    type Output = Result<Response<ResBody>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.response_future.poll(cx) {
            Poll::Ready(Ok(res)) => {
                let latency = this.start.elapsed();
                let status = res.status().as_u16();
                let path = this.path.to_string();

                (this.metrics_callback)(path, latency, status);

                Poll::Ready(Ok(res))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }
}
