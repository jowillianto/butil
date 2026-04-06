use super::prelude::IntoValue;
use crate::cache_store::JsonTF;
use std::ops::Deref;

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
    tc: TC,
    fc: FC,
}

impl<
    'a,
    N,
    C,
    S: Service<Cache = C> + ?Sized,
    TC: ToCache<Native = N, Cache = C>,
    FC: FromCache<Native = N, Cache = C>,
> Value<'a, N, C, S, TC, FC>
{
    pub async fn new(
        k: impl Into<String>,
        value: N,
        service: &'a S,
        tc: TC,
        fc: FC,
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
        tc: TC,
        fc: FC,
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
}

impl<
    'a,
    N,
    C,
    S: Service<Cache = C> + ?Sized,
    TC: ToCache<Native = N, Cache = C> + Clone,
    FC: FromCache<Native = N, Cache = C> + Clone,
> IntoValue for Value<'a, N, C, S, TC, FC>
{
    type Target = N;
    fn into_value(self) -> N {
        self.value
    }
}

impl<
    'a,
    N,
    C,
    S: Service<Cache = C> + ?Sized,
    TC: ToCache<Native = N, Cache = C> + Clone,
    FC: FromCache<Native = N, Cache = C> + Clone,
> Value<'a, N, C, S, TC, FC>
{
    pub async fn refresh(self) -> Result<Self, Error> {
        match Self::get(
            self.key.as_str(),
            self.service,
            self.tc.clone(),
            self.fc.clone(),
        )
        .await?
        {
            None => Self::new(self.key, self.value, self.service, self.tc, self.fc).await,
            Some(v) => Ok(v),
        }
    }
}

impl<
    'a,
    N,
    C,
    S: Service<Cache = C> + ?Sized,
    TC: ToCache<Native = N, Cache = C>,
    FC: FromCache<Native = N, Cache = C>,
> Deref for Value<'a, N, C, S, TC, FC>
{
    type Target = N;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<
    'a,
    N: serde::Serialize + for<'de> serde::Deserialize<'de>,
    S: Service<Cache = String> + ?Sized,
> Value<'a, N, String, S, JsonTF<N>, JsonTF<N>>
{
    pub async fn new_json(k: impl Into<String>, value: N, service: &'a S) -> Result<Self, Error> {
        Self::new(
            k,
            value,
            service,
            JsonTF::<N>::default(),
            JsonTF::<N>::default(),
        )
        .await
    }
    pub async fn get_json(k: impl Into<String>, service: &'a S) -> Result<Option<Self>, Error> {
        Self::get(k, service, JsonTF::<N>::default(), JsonTF::<N>::default()).await
    }
}

pub async fn new_json_value<
    'a,
    N: serde::Serialize + for<'de> serde::Deserialize<'de>,
    S: Service<Cache = String> + ?Sized,
>(
    k: impl Into<String>,
    value: N,
    service: &'a S,
) -> Result<Value<'a, N, String, S, JsonTF<N>, JsonTF<N>>, Error> {
    Value::<'a, N, String, S, JsonTF<N>, JsonTF<N>>::new_json(k, value, service).await
}

pub async fn get_json_value<
    'a,
    N: serde::Serialize + for<'de> serde::Deserialize<'de>,
    S: Service<Cache = String> + ?Sized,
>(
    k: impl Into<String>,
    service: &'a S,
) -> Result<Option<Value<'a, N, String, S, JsonTF<N>, JsonTF<N>>>, Error> {
    Value::<'a, N, String, S, JsonTF<N>, JsonTF<N>>::get_json(k, service).await
}
