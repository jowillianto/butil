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
