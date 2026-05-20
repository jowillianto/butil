use std::future::Future;

pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    pub fn is_left(&self) -> bool {
        matches!(self, Self::Left(_))
    }
    pub fn is_right(&self) -> bool {
        matches!(self, Self::Right(_))
    }
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

pub enum Either3<A, B, C> {
    Left(A),
    Center(B),
    Right(C),
}

impl<A, B, C> Either3<A, B, C> {
    pub async fn wait<FA, FB, FC>(a: FA, b: FB, c: FC) -> Either3<A, B, C>
    where
        FA: Future<Output = A>,
        FB: Future<Output = B>,
        FC: Future<Output = C>,
    {
        tokio::select! {
          v = a => Either3::Left(v),
          v = b => Either3::Center(v),
          v = c => Either3::Right(v),
        }
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
