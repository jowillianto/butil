use super::error::Error;
use super::prelude::*;

pub struct Value<
    'a,
    N,
    C,
    S: Service<Cache = C> + ?Sized,
    TC: ToCache<Native = N, Cache = C>,
    FC: FromCache<Native = N, Cache = C>,
> {
    value: N,
    key: String,
    service: &'a S,
    tc: &'a TC,
    fc: &'a FC,
}

impl<'a, N, C, S, TC, FC> Value<'a, N, C, S, TC, FC>
where
    S: Service<Cache = C> + ?Sized,
    TC: ToCache<Native = N, Cache = C>,
    FC: FromCache<Native = N, Cache = C>,
{
    pub async fn new(
        k: impl Into<String>,
        value: N,
        service: &'a S,
        tc: &'a TC,
        fc: &'a FC,
    ) -> Result<Self, Error> {
        let key = k.into();
        let converted_value = tc.to_cache(&value)?;
        service.set(&key, &converted_value).await?;
        Ok(Self {
            key,
            value,
            service,
            tc,
            fc,
        })
    }

    pub async fn get(
        k: impl Into<String>,
        service: &'a S,
        tc: &'a TC,
        fc: &'a FC,
    ) -> Result<Option<Self>, Error> {
        let key = k.into();
        match service.get(&key).await? {
            None => Ok(None),
            Some(v) => {
                let value = fc.from_cache(&v)?;
                Ok(Some(Self {
                    key,
                    value,
                    service,
                    tc,
                    fc,
                }))
            }
        }
    }

    pub async fn update(self, v: N) -> Result<Self, Error> {
        Self::new(self.key, v, self.service, self.tc, self.fc).await
    }

    pub async fn del(self) -> Result<bool, Error> {
        self.service.del(self.key.as_str()).await
    }

    pub async fn refresh(self) -> Result<Self, Error> {
        match Self::get(self.key.as_str(), self.service, self.tc, self.fc).await? {
            None => Self::new(self.key, self.value, self.service, self.tc, self.fc).await,
            Some(v) => Ok(v),
        }
    }

    pub async fn mutate<O: Future<Output = Result<(), Error>>, F: Fn(&mut N) -> O>(
        &mut self,
        f: F,
    ) -> Result<(), Error> {
        f(&mut self.value).await?;
        self.service
            .set(self.key.as_str(), &self.tc.to_cache(&self.value)?)
            .await?;
        Ok(())
    }

    pub async fn value(&self) -> &N {
        &self.value
    }
}
