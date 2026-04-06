use std::future::Future;

pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    pub async fn wait<LF: Future<Output = L>, RF: Future<Output = R>>(
        l: LF,
        r: RF,
    ) -> Either<L, R> {
        tokio::select! {
          v = l => Either::Left(v),
          v = r => Either::Right(v)
        }
    }
}

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
