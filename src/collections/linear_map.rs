pub struct LinearMap<K, V> {
    inner: Vec<(K, V)>,
}

impl<K, V> LinearMap<K, V> {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }
    pub fn new_with_capacity(size: usize) -> Self {
        Self {
            inner: Vec::with_capacity(size),
        }
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn insert(&mut self, k: impl Into<K> + PartialEq<K>, v: impl Into<V>) -> bool {
        if self.inner.iter().any(|(key, _)| k.eq(key)) {
            return false;
        }
        self.inner.push((k.into(), v.into()));
        true
    }
    pub fn filter_self(&mut self, f: impl FnMut(&(K, V)) -> bool) {
        self.inner = std::mem::take::<Vec<(K, V)>>(&mut self.inner)
            .into_iter()
            .filter(f)
            .collect();
    }
    pub fn insert_no_check(&mut self, k: impl Into<K>, v: impl Into<V>) -> &mut V {
        self.inner.push((k.into(), v.into()));
        &mut self.inner.iter_mut().last().unwrap().1
    }
    pub fn get(&self, k: &(impl PartialEq<K> + ?Sized)) -> Option<&V> {
        self.inner
            .iter()
            .find(|(key, _)| k.eq(key))
            .map(|(_, value)| value)
    }
    pub fn get_mut(&mut self, k: &(impl PartialEq<K> + ?Sized)) -> Option<&mut V> {
        self.inner
            .iter_mut()
            .find(|(key, _)| k.eq(key))
            .map(|(_, value)| value)
    }
    pub fn get_by_id(&self, id: usize) -> Option<(&K, &V)> {
        self.inner.get(id).map(|(key, value)| (key, value))
    }
    pub fn get_mut_by_id(&mut self, id: usize) -> Option<(&mut K, &mut V)> {
        self.inner.get_mut(id).map(|(key, value)| (key, value))
    }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter().map(|(key, value)| (key, value))
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&mut K, &mut V)> {
        self.inner.iter_mut().map(|(key, value)| (key, value))
    }
    pub fn iter_keys(&self) -> impl Iterator<Item = &K> {
        self.inner.iter().map(|(key, _)| key)
    }
    pub fn remove(&mut self, k: &(impl PartialEq<K> + ?Sized)) -> Option<V> {
        let id = self.inner.iter().position(|(key, _)| k.eq(key))?;
        Some(self.inner.remove(id).1)
    }
    pub fn remove_by_id(&mut self, id: usize) -> Option<V> {
        if id >= self.inner.len() {
            return None;
        }
        Some(self.inner.remove(id).1)
    }
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<K: Clone, V: Clone> Clone for LinearMap<K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<K: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for LinearMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinearMap")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<K, V> Default for LinearMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> IntoIterator for LinearMap<K, V> {
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<(K, V)>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

unsafe impl<K: Send, V: Send> Send for LinearMap<K, V> {}
unsafe impl<K: Sync, V: Sync> Sync for LinearMap<K, V> {}

impl<K: prost::Message + Default, V: prost::Message + Default> prost::Message for LinearMap<K, V> {
    fn encode_raw(&self, buf: &mut impl prost::bytes::BufMut) {
        for (key, value) in self.inner.iter() {
            let len = prost::encoding::message::encoded_len(1, key)
                + prost::encoding::message::encoded_len(2, value);

            prost::encoding::encode_key(1, prost::encoding::WireType::LengthDelimited, buf);
            prost::encoding::encode_varint(len as u64, buf);
            prost::encoding::message::encode(1, key, buf);
            prost::encoding::message::encode(2, value, buf);
        }
    }

    fn merge_field(
        &mut self,
        tag: u32,
        wire_type: prost::encoding::WireType,
        buf: &mut impl prost::bytes::Buf,
        ctx: prost::encoding::DecodeContext,
    ) -> Result<(), prost::DecodeError> {
        match tag {
            1 => {
                prost::encoding::check_wire_type(
                    prost::encoding::WireType::LengthDelimited,
                    wire_type,
                )?;

                let mut key = K::default();
                let mut value = V::default();

                prost::encoding::merge_loop(
                    &mut (&mut key, &mut value),
                    buf,
                    ctx,
                    |state, buf, ctx| {
                        let (tag, wire_type) = prost::encoding::decode_key(buf)?;
                        match tag {
                            1 => prost::encoding::message::merge(wire_type, state.0, buf, ctx),
                            2 => prost::encoding::message::merge(wire_type, state.1, buf, ctx),
                            _ => prost::encoding::skip_field(wire_type, tag, buf, ctx),
                        }
                    },
                )?;

                self.insert_no_check(key, value);
                Ok(())
            }
            _ => prost::encoding::skip_field(wire_type, tag, buf, ctx),
        }
    }

    fn encoded_len(&self) -> usize {
        self.inner
            .iter()
            .map(|(key, value)| {
                let len = prost::encoding::message::encoded_len(1, key)
                    + prost::encoding::message::encoded_len(2, value);
                prost::encoding::key_len(1) + prost::encoding::encoded_len_varint(len as u64) + len
            })
            .sum()
    }

    fn clear(&mut self) {
        self.inner.clear();
    }
}

pub fn linear_map_to_map<K: serde::Serialize, V: serde::Serialize, S: serde::Serializer>(
    m: &LinearMap<K, V>,
    s: S,
) -> Result<S::Ok, S::Error> {
    s.collect_map(m.iter())
}

pub fn linear_map_to_seq<K: serde::Serialize, V: serde::Serialize, S: serde::Serializer>(
    m: &LinearMap<K, V>,
    s: S,
) -> Result<S::Ok, S::Error> {
    s.collect_seq(m.iter())
}

struct MapVisitor<K, V>(std::marker::PhantomData<(K, V)>);

impl<'de, K: serde::Deserialize<'de>, V: serde::Deserialize<'de>> serde::de::Visitor<'de>
    for MapVisitor<K, V>
{
    type Value = LinearMap<K, V>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a map")
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut m = LinearMap::new_with_capacity(a.size_hint().unwrap_or(0));
        while let Some((k, v)) = a.next_entry::<K, V>()? {
            m.insert_no_check(k, v);
        }
        Ok(m)
    }
}

struct SeqVisitor<K, V>(std::marker::PhantomData<(K, V)>);

impl<'de, K: serde::Deserialize<'de>, V: serde::Deserialize<'de>> serde::de::Visitor<'de>
    for SeqVisitor<K, V>
{
    type Value = LinearMap<K, V>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a sequence of key value pairs")
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut m = LinearMap::new_with_capacity(a.size_hint().unwrap_or(0));
        while let Some((k, v)) = a.next_element::<(K, V)>()? {
            m.insert_no_check(k, v);
        }
        Ok(m)
    }
}

pub fn linear_map_from_map<'de, K, V, D>(d: D) -> Result<LinearMap<K, V>, D::Error>
where
    K: serde::Deserialize<'de>,
    V: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    d.deserialize_map(MapVisitor(std::marker::PhantomData))
}

pub fn linear_map_from_seq<'de, K, V, D>(d: D) -> Result<LinearMap<K, V>, D::Error>
where
    K: serde::Deserialize<'de>,
    V: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    d.deserialize_seq(SeqVisitor(std::marker::PhantomData))
}
