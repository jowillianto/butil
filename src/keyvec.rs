pub struct KeyVec<K, V> {
    inner: Vec<(K, V)>,
}

impl<K, V> KeyVec<K, V> {
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
    pub fn insert_no_check(&mut self, k: impl Into<K>, v: impl Into<V>) {
        self.inner.push((k.into(), v.into()));
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

impl<K: Clone, V: Clone> Clone for KeyVec<K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<K: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for KeyVec<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyVec")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<K, V> Default for KeyVec<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<K: Send, V: Send> Send for KeyVec<K, V> {}
unsafe impl<K: Sync, V: Sync> Sync for KeyVec<K, V> {}

pub struct BoundedKeyVec<K, V> {
    inner: KeyVec<K, V>,
    max_size: usize,
}

impl<K, V> BoundedKeyVec<K, V> {
    pub fn new(max_size: usize) -> Self {
        assert!(
            max_size > 0,
            "BoundedKeyVec max_size must be greater than 0"
        );
        Self {
            inner: KeyVec::new_with_capacity(max_size),
            max_size,
        }
    }

    pub fn new_with_capacity(max_size: usize, size: usize) -> Self {
        assert!(
            max_size > 0,
            "BoundedKeyVec max_size must be greater than 0"
        );
        Self {
            inner: KeyVec::new_with_capacity(size),
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

impl<K: Clone, V: Clone> Clone for BoundedKeyVec<K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            max_size: self.max_size,
        }
    }
}

impl<K: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for BoundedKeyVec<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundedKeyVec")
            .field("max_size", &self.max_size)
            .field("inner", &self.inner)
            .finish()
    }
}

unsafe impl<K: Send, V: Send> Send for BoundedKeyVec<K, V> {}
unsafe impl<K: Sync, V: Sync> Sync for BoundedKeyVec<K, V> {}

impl<K: prost::Message + Default, V: prost::Message + Default> prost::Message for KeyVec<K, V> {
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
