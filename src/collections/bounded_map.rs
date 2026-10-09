use super::linear_map::LinearMap;

pub struct BoundedMap<K, V> {
    inner: LinearMap<K, V>,
    max_size: usize,
}

impl<K, V> BoundedMap<K, V> {
    pub fn new(max_size: usize) -> Self {
        assert!(max_size > 0, "BoundedMap max_size must be greater than 0");
        Self {
            inner: LinearMap::new_with_capacity(max_size),
            max_size,
        }
    }

    pub fn new_with_capacity(max_size: usize, size: usize) -> Self {
        assert!(max_size > 0, "BoundedMap max_size must be greater than 0");
        Self {
            inner: LinearMap::new_with_capacity(size),
            max_size,
        }
    }

    pub fn max_size(&self) -> usize {
        self.max_size
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn insert(&mut self, k: impl Into<K>, v: impl Into<V>) -> bool
    where
        K: PartialEq<K>,
    {
        let k = k.into();
        if self.inner.get(&k).is_some() {
            return false;
        }
        if self.inner.len() >= self.max_size {
            let _ = self.inner.remove_by_id(0);
        }
        self.inner.insert_no_check(k, v);
        true
    }

    pub fn insert_no_check(&mut self, k: impl Into<K>, v: impl Into<V>) {
        if self.inner.len() >= self.max_size {
            let _ = self.inner.remove_by_id(0);
        }
        self.inner.insert_no_check(k, v);
    }

    pub fn filter_self(&mut self, f: impl FnMut(&(K, V)) -> bool) {
        self.inner.filter_self(f);
    }

    pub fn get(&self, k: &(impl PartialEq<K> + ?Sized)) -> Option<&V> {
        self.inner.get(k)
    }

    pub fn get_mut(&mut self, k: &(impl PartialEq<K> + ?Sized)) -> Option<&mut V> {
        self.inner.get_mut(k)
    }

    pub fn get_by_id(&self, id: usize) -> Option<(&K, &V)> {
        self.inner.get_by_id(id)
    }

    pub fn get_mut_by_id(&mut self, id: usize) -> Option<(&mut K, &mut V)> {
        self.inner.get_mut_by_id(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&mut K, &mut V)> {
        self.inner.iter_mut()
    }

    pub fn iter_keys(&self) -> impl Iterator<Item = &K> {
        self.inner.iter_keys()
    }

    pub fn remove(&mut self, k: &(impl PartialEq<K> + ?Sized)) -> Option<V> {
        self.inner.remove(k)
    }

    pub fn remove_by_id(&mut self, id: usize) -> Option<V> {
        self.inner.remove_by_id(id)
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<K: Clone, V: Clone> Clone for BoundedMap<K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            max_size: self.max_size,
        }
    }
}

impl<K: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for BoundedMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundedMap")
            .field("max_size", &self.max_size)
            .field("inner", &self.inner)
            .finish()
    }
}

unsafe impl<K: Send, V: Send> Send for BoundedMap<K, V> {}
unsafe impl<K: Sync, V: Sync> Sync for BoundedMap<K, V> {}

struct AsMap<'a, K, V>(&'a LinearMap<K, V>);

impl<K: serde::Serialize, V: serde::Serialize> serde::Serialize for AsMap<'_, K, V> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        super::linear_map::linear_map_to_map(self.0, s)
    }
}

struct AsSeq<'a, K, V>(&'a LinearMap<K, V>);

impl<K: serde::Serialize, V: serde::Serialize> serde::Serialize for AsSeq<'_, K, V> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        super::linear_map::linear_map_to_seq(self.0, s)
    }
}

#[derive(serde::Deserialize)]
#[serde(bound(deserialize = "K: serde::Deserialize<'de>, V: serde::Deserialize<'de>"))]
struct RawMap<K, V> {
    max_size: usize,
    #[serde(deserialize_with = "super::linear_map::linear_map_from_map")]
    inner: LinearMap<K, V>,
}

#[derive(serde::Deserialize)]
#[serde(bound(deserialize = "K: serde::Deserialize<'de>, V: serde::Deserialize<'de>"))]
struct RawSeq<K, V> {
    max_size: usize,
    #[serde(deserialize_with = "super::linear_map::linear_map_from_seq")]
    inner: LinearMap<K, V>,
}

fn build<K, V, E: serde::de::Error>(
    max_size: usize,
    inner: LinearMap<K, V>,
) -> Result<BoundedMap<K, V>, E> {
    if max_size == 0 {
        return Err(E::custom("BoundedMap max_size must be greater than 0"));
    }
    let mut m = BoundedMap::new(max_size);
    for (k, v) in inner {
        m.insert_no_check(k, v);
    }
    Ok(m)
}

pub fn bounded_map_to_map<K: serde::Serialize, V: serde::Serialize, S: serde::Serializer>(
    m: &BoundedMap<K, V>,
    s: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeStruct;
    let mut st = s.serialize_struct("BoundedMap", 2)?;
    st.serialize_field("max_size", &m.max_size)?;
    st.serialize_field("inner", &AsMap(&m.inner))?;
    st.end()
}

pub fn bounded_map_to_seq<K: serde::Serialize, V: serde::Serialize, S: serde::Serializer>(
    m: &BoundedMap<K, V>,
    s: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeStruct;
    let mut st = s.serialize_struct("BoundedMap", 2)?;
    st.serialize_field("max_size", &m.max_size)?;
    st.serialize_field("inner", &AsSeq(&m.inner))?;
    st.end()
}

pub fn bounded_map_from_map<'de, K, V, D>(d: D) -> Result<BoundedMap<K, V>, D::Error>
where
    K: serde::Deserialize<'de>,
    V: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    let raw = <RawMap<K, V> as serde::Deserialize>::deserialize(d)?;
    build(raw.max_size, raw.inner)
}

pub fn bounded_map_from_seq<'de, K, V, D>(d: D) -> Result<BoundedMap<K, V>, D::Error>
where
    K: serde::Deserialize<'de>,
    V: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    let raw = <RawSeq<K, V> as serde::Deserialize>::deserialize(d)?;
    build(raw.max_size, raw.inner)
}
