use std::future::Future;

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
