pub mod either;
pub mod either3;
pub mod worker;

use std::future::Future;

pub use either::Either;
pub use either3::Either3;
pub use worker::{Worker, WorkerArg};

pub async fn wait_or<L: Future, R: Future<Output = ()>>(l: L, r: R) -> Option<L::Output> {
    match Either::wait(l, r).await {
        Either::Left(v) => Some(v),
        Either::Right(_) => None,
    }
}

pub async fn wait_for<L: Future>(l: L, dur: tokio::time::Duration) -> Option<L::Output> {
    match Either::wait(l, tokio::time::sleep(dur)).await {
        Either::Left(v) => Some(v),
        Either::Right(_) => None,
    }
}

pub async fn wait_or_option<O, L: Future<Output = Option<O>>, R: Future<Output = ()>>(
    l: L,
    r: R,
) -> Option<O> {
    match Either::wait(l, r).await {
        Either::Left(v) => v,
        Either::Right(_) => None,
    }
}

pub async fn wait_or3<A, B, FA, FB, FC>(a: FA, b: FB, cancel: FC) -> Option<Either<A, B>>
where
    FA: Future<Output = A>,
    FB: Future<Output = B>,
    FC: Future<Output = ()>,
{
    match Either3::wait(a, b, cancel).await {
        Either3::Left(v) => Some(Either::Left(v)),
        Either3::Center(v) => Some(Either::Right(v)),
        Either3::Right(_) => None,
    }
}
