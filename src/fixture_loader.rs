use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ConnectionTrait, EntityTrait, IntoActiveModel,
    ModelTrait, SelectExt,
};
use std::{collections::HashMap, ops::Deref};

#[async_trait::async_trait]
pub trait LoadFixture<C: ConnectionTrait, Err>: Send + Sync {
    async fn insert_or_update_fixture(&self, v: toml::Value, c: &C) -> Result<(), Err>;
}

pub struct AutoLoadFixture<E> {
    entity: std::marker::PhantomData<E>,
}

impl<E> AutoLoadFixture<E> {
    pub fn new() -> Self {
        Self {
            entity: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<
    C: ConnectionTrait,
    Err: From<toml::de::Error> + From<sea_orm::DbErr>,
    A: ActiveModelTrait<Entity = E> + Send + ActiveModelBehavior,
    E: EntityTrait<
            Model: for<'de> serde::Deserialize<'de> + IntoActiveModel<A> + ModelTrait,
            ActiveModel = A,
        >,
> LoadFixture<C, Err> for AutoLoadFixture<E>
{
    async fn insert_or_update_fixture(&self, v: toml::Value, c: &C) -> Result<(), Err> {
        let model = v.try_into::<E::Model>()?;
        let query = E::find_by_id(<<E::PrimaryKey as sea_orm::PrimaryKeyTrait>::ValueType as sea_orm::sea_query::FromValueTuple>::from_value_tuple(
            sea_orm::ModelTrait::get_primary_key_value(&model),
        ));
        let exists = query.exists(c).await?;
        let active_model = model.into_active_model().reset_all();
        if exists {
            active_model.update(c).await?;
        } else {
            active_model.insert(c).await?;
        }
        Ok(())
    }
}

pub struct FixtureLoader<C: ConnectionTrait, Err> {
    loaders: HashMap<String, Box<dyn LoadFixture<C, Err>>>,
}

impl<C: ConnectionTrait, Err> FixtureLoader<C, Err> {
    pub fn new() -> Self {
        Self {
            loaders: HashMap::new(),
        }
    }
    pub fn add_loader<L: 'static + LoadFixture<C, Err> + Send>(
        mut self,
        name: impl Into<String>,
        entity: L,
    ) -> Self {
        self.loaders.insert(name.into(), Box::new(entity));
        self
    }
    pub fn get_loader(&self, name: &str) -> Option<&dyn LoadFixture<C, Err>> {
        self.loaders.get(name).map(|l| l.deref())
    }
}
