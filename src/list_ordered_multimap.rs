use dlv_list::{
    Drain as VecListDrain, Index, IntoIter as VecListIntoIter, Iter as VecListIter,
    IterMut as VecListIterMut, VecList,
};
use hashbrown::hash_map::Entry as HashMapEntry;
use hashbrown::HashMap;
use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::fmt::{self, Debug, Formatter};
use std::hash::{BuildHasher, Hash, Hasher};
use std::iter::{FromIterator, FusedIterator};
use std::marker::PhantomData;

#[derive(Clone)]
pub struct ListOrderedMultimap<Key, Value, State = RandomState> {
    /// The list of the keys in the multimap. This is ordered by time of insertion.
    keys: VecList<Key>,

    /// The map from hashes of keys to the indices of their values in the value list. The list of
    /// the indices is ordered by time of insertion.
    map: HashMap<KeyHash, MapEntry<Key, Value>, State>,

    /// The list of the values in the multimap. This is ordered by time of insertion.
    values: VecList<ValueEntry<Key, Value>>,
}

impl<Key, Value> ListOrderedMultimap<Key, Value, RandomState>
where
    Key: Eq + Hash,
{
    /// Creates a new multimap with no initial capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key1", "value1");
    /// assert_eq!(map.get(&"key1"), Some(&"value1"));
    /// ```
    pub fn new() -> ListOrderedMultimap<Key, Value, RandomState> {
        ListOrderedMultimap::default()
    }

    /// Creates a new multimap with the specified capacities.
    ///
    /// The multimap will be able to hold at least `key_capacity` keys and `value_capacity` values
    /// without reallocating. A capacity of 0 will result in no allocation for the respective
    /// container.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::new();
    /// assert_eq!(map.keys_capacity(), 0);
    /// assert_eq!(map.values_capacity(), 0);
    ///
    /// let map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::with_capacity(5, 10);
    /// assert_eq!(map.keys_capacity(), 5);
    /// assert_eq!(map.values_capacity(), 10);
    /// ```
    pub fn with_capacity(
        key_capacity: usize,
        value_capacity: usize,
    ) -> ListOrderedMultimap<Key, Value, RandomState> {
        ListOrderedMultimap {
            keys: VecList::with_capacity(key_capacity),
            map: HashMap::with_capacity_and_hasher(key_capacity, RandomState::new()),
            values: VecList::with_capacity(value_capacity),
        }
    }
}

impl<Key, Value, State> ListOrderedMultimap<Key, Value, State>
where
    Key: Eq + Hash,
    State: BuildHasher,
{
    /// Appends a value to the list of values associated with the given key.
    ///
    /// If the key is not already in the multimap, this will be identical to an insert and the
    /// return value will be `false`. Otherwise, `true` will be returned.
    ///
    /// Complexity: amortized O(1)
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// let already_exists = map.append("key", "value");
    /// assert!(!already_exists);
    /// assert_eq!(map.values_len(), 1);
    /// assert_eq!(map.get(&"key"), Some(&"value"));
    ///
    /// let already_exists = map.append("key", "value2");
    /// assert!(already_exists);
    /// assert_eq!(map.values_len(), 2);
    /// ```
    pub fn append(&mut self, key: Key, value: Value) -> bool {
        use self::HashMapEntry::*;

        let hash = self.key_hash(&key);

        match self.map.entry(hash) {
            Occupied(mut entry) => {
                let map_entry = entry.get_mut();
                let value_entry = ValueEntry::new(map_entry.key_index, value);
                let index = self.values.push_back(value_entry);
                self.values
                    .get_mut(map_entry.tail_index)
                    .unwrap()
                    .next_index = Some(index);
                map_entry.append(index);
                true
            }
            Vacant(entry) => {
                let key_index = self.keys.push_back(key);
                let value_entry = ValueEntry::new(key_index, value);
                let index = self.values.push_back(value_entry);
                entry.insert(MapEntry::new(key_index, index));
                false
            }
        }
    }

    /// Removes all keys and values from the multimap.
    ///
    /// Complexity: O(|K| + |V|) where |K| is the number of keys and |V| is the number of values.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value");
    /// assert_eq!(map.keys_len(), 1);
    /// assert_eq!(map.values_len(), 1);
    ///
    /// map.clear();
    /// assert_eq!(map.keys_len(), 0);
    /// assert_eq!(map.values_len(), 0);
    /// ```
    pub fn clear(&mut self) {
        self.keys.clear();
        self.map.clear();
        self.values.clear();
    }

    /// Returns whether the given key is in the multimap.
    ///
    /// Complexity: O(1)
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// assert!(!map.contains_key(&"key"));
    /// map.insert("key", "value");
    /// assert!(map.contains_key(&"key"));
    /// ```
    pub fn contains_key<KeyQuery>(&self, key: &KeyQuery) -> bool
    where
        Key: Borrow<KeyQuery>,
        KeyQuery: ?Sized + Eq + Hash,
    {
        self.map.contains_key(&self.key_hash(key))
    }

    pub fn drain(&mut self) -> Drain<Key, Value, State> {
        Drain {
            iter: self.values.drain(),
            keys: &mut self.keys,
            map: &mut self.map,
        }
    }

    /// Returns whether the given key is in the multimap.
    ///
    /// Complexity: O(1)
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// let value = map.entry("key").or_insert("value");
    /// assert_eq!(value, &"value");
    /// assert_eq!(map.get(&"key"), Some(&"value"));
    /// ```
    pub fn entry(&mut self, key: Key) -> Entry<Key, Value, State> {
        let hash = self.key_hash(&key);

        if self.map.contains_key(&hash) {
            Entry::Occupied(OccupiedEntry { hash, map: self })
        } else {
            Entry::Vacant(VacantEntry {
                hash,
                key,
                map: self,
            })
        }
    }

    /// Returns the number of values associated with a key.
    ///
    /// Complexity: O(1)
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// assert_eq!(map.entry_len(&"key"), 0);
    ///
    /// map.insert("key", "value1");
    /// assert_eq!(map.entry_len(&"key"), 1);
    ///
    /// map.append(&"key", "value2");
    /// assert_eq!(map.entry_len(&"key"), 2);
    /// ```
    pub fn entry_len<KeyQuery>(&self, key: &KeyQuery) -> usize
    where
        Key: Borrow<KeyQuery>,
        KeyQuery: ?Sized + Eq + Hash,
    {
        let hash = self.key_hash(key);
        self.map
            .get(&hash)
            .map(|map_entry| map_entry.length)
            .unwrap_or(0)
    }

    /// Returns an immutable reference to the first value, by insertion order, associated with the
    /// given key, or `None` if the key is not in the multimap.
    ///
    /// Complexity: O(1)
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// assert_eq!(map.get(&"key"), None);
    ///
    /// map.insert("key", "value");
    /// assert_eq!(map.get(&"key"), Some(&"value"));
    ///
    /// map.append("key", "value2");
    /// assert_eq!(map.get(&"key"), Some(&"value"));
    /// ```
    pub fn get<KeyQuery>(&self, key: &KeyQuery) -> Option<&Value>
    where
        Key: Borrow<KeyQuery>,
        KeyQuery: ?Sized + Eq + Hash,
    {
        let hash = self.key_hash(key);
        let index = self.map.get(&hash)?.head_index;
        self.values.get(index).map(|entry| &entry.value)
    }

    /// Returns an iterator that yields immutable references to all values associated with the
    /// given key by insertion order.
    ///
    /// If the key is not in the multimap, the iterator will yield no values.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value");
    /// map.append("key", "value2");
    ///
    /// let mut iter = map.get_all(&"key");
    /// assert_eq!(iter.next(), Some(&"value"));
    /// assert_eq!(iter.next(), Some(&"value2"));
    /// assert_eq!(iter.next(), None);
    /// ```
    pub fn get_all<KeyQuery>(&self, key: &KeyQuery) -> EntryValues<Key, Value>
    where
        Key: Borrow<KeyQuery>,
        KeyQuery: ?Sized + Eq + Hash,
    {
        let hash = self.key_hash(key);

        match self.map.get(&hash) {
            Some(map_entry) => EntryValues::from_map_entry(&self.values, &map_entry),
            None => EntryValues::empty(&self.values),
        }
    }

    /// Returns an iterator that yields mutable references to all values associated with the given
    /// key by insertion order.
    ///
    /// If the key is not in the multimap, the iterator will yield no values.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    /// map.append("key", "value2");
    ///
    /// let mut iter = map.get_all_mut(&"key");
    ///
    /// let first = iter.next().unwrap();
    /// assert_eq!(first, &mut "value1");
    /// *first = "value3";
    ///
    /// assert_eq!(iter.next(), Some(&mut "value2"));
    /// assert_eq!(iter.next(), None);
    ///
    /// assert_eq!(map.get(&"key"), Some(&"value3"));
    /// ```
    pub fn get_all_mut<KeyQuery>(&mut self, key: &KeyQuery) -> EntryValuesMut<Key, Value>
    where
        Key: Borrow<KeyQuery>,
        KeyQuery: ?Sized + Eq + Hash,
    {
        let hash = self.key_hash(key);

        match self.map.get(&hash) {
            Some(map_entry) => EntryValuesMut::from_map_entry(&mut self.values, &map_entry),
            None => EntryValuesMut::empty(&mut self.values),
        }
    }

    /// Returns a mutable reference to the first value, by insertion order, associated with the
    /// given key, or `None` if the key is not in the multimap.
    ///
    /// Complexity: O(1)
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// assert_eq!(map.get(&"key"), None);
    ///
    /// map.insert("key", "value");
    /// assert_eq!(map.get(&"key"), Some(&"value"));
    ///
    /// let mut value = map.get_mut(&"key").unwrap();
    /// *value = "value2";
    ///
    /// assert_eq!(map.get(&"key"), Some(&"value2"));
    /// ```
    pub fn get_mut<KeyQuery>(&mut self, key: &KeyQuery) -> Option<&mut Value>
    where
        Key: Borrow<KeyQuery>,
        KeyQuery: ?Sized + Eq + Hash,
    {
        let hash = self.key_hash(key);
        let index = self.map.get(&hash)?.head_index;
        self.values.get_mut(index).map(|entry| &mut entry.value)
    }

    /// Returns a reference to the multimap's [`BuildHasher`].
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::new();
    /// let hasher = map.hasher();
    /// ```
    pub fn hasher(&self) -> &State {
        self.map.hasher()
    }

    /// Inserts the key-value pair into the multimap and returns the first value, by insertion
    /// order, that was already associated with the key.
    ///
    /// If the key is not already in the multimap, `None` will be returned.
    ///
    /// Complexity: O(1) amortized
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// assert!(map.is_empty());
    ///
    /// let old_value = map.insert("key", "value");
    /// assert!(old_value.is_none());
    /// assert_eq!(map.values_len(), 1);
    /// assert_eq!(map.get(&"key"), Some(&"value"));
    ///
    /// let old_value = map.insert("key", "value2");
    /// assert_eq!(old_value, Some("value"));
    /// assert_eq!(map.values_len(), 1);
    /// assert_eq!(map.get(&"key"), Some(&"value2"));
    /// ```
    pub fn insert(&mut self, key: Key, value: Value) -> Option<Value> {
        self.insert_all(key, value).next()
    }

    /// Inserts the key-value pair into the multimap and returns an iterator that yields all values
    /// previously associated with the key by insertion order.
    ///
    /// If the key is not already in the multimap, the iterator will yield no values.
    ///
    /// Complexity: O(1) amortized
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// assert!(map.is_empty());
    ///
    /// {
    ///     let mut old_values = map.insert_all("key", "value");
    ///     assert_eq!(old_values.next(), None);
    /// }
    ///
    /// assert_eq!(map.values_len(), 1);
    /// assert_eq!(map.get(&"key"), Some(&"value"));
    ///
    /// map.append("key", "value2");
    ///
    /// {
    ///     let mut old_values = map.insert_all("key", "value3");
    ///     assert_eq!(old_values.next(), Some("value"));
    ///     assert_eq!(old_values.next(), Some("value2"));
    ///     assert_eq!(old_values.next(), None);
    /// }
    ///
    /// assert_eq!(map.values_len(), 1);
    /// assert_eq!(map.get(&"key"), Some(&"value3"));
    /// ```
    pub fn insert_all(&mut self, key: Key, value: Value) -> EntryValuesDrain<Key, Value> {
        use self::HashMapEntry::*;

        let hash = self.key_hash(&key);

        match self.map.entry(hash) {
            Occupied(mut entry) => {
                let map_entry = entry.get_mut();
                let value_entry = ValueEntry::new(map_entry.key_index, value);
                let index = self.values.push_back(value_entry);
                let iter = EntryValuesDrain::from_map_entry(&mut self.values, &map_entry);
                map_entry.reset(index);
                iter
            }
            Vacant(entry) => {
                let key_index = self.keys.push_back(key);
                let value_entry = ValueEntry::new(key_index, value);
                let index = self.values.push_back(value_entry);
                entry.insert(MapEntry::new(key_index, index));
                EntryValuesDrain::empty(&mut self.values)
            }
        }
    }

    /// Returns whether the multimap is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// assert!(map.is_empty());
    ///
    /// map.insert("key1", "value");
    /// assert!(!map.is_empty());
    ///
    /// map.remove(&"key1");
    /// assert!(map.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Returns an iterator that yields immutable references to all key-value pairs in the multimap
    /// by insertion order.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key1", "value1");
    /// map.insert("key2", "value1");
    /// map.append(&"key1", "value2");
    /// map.append(&"key2", "value2");
    ///
    /// let mut iter = map.iter();
    /// assert_eq!(iter.size_hint(), (4, Some(4)));
    /// assert_eq!(iter.next(), Some((&"key1", &"value1")));
    /// assert_eq!(iter.next(), Some((&"key2", &"value1")));
    /// assert_eq!(iter.next(), Some((&"key1", &"value2")));
    /// assert_eq!(iter.next(), Some((&"key2", &"value2")));
    /// assert_eq!(iter.next(), None);
    /// ```
    pub fn iter(&self) -> Iter<Key, Value> {
        Iter {
            keys: &self.keys,
            iter: self.values.iter(),
        }
    }

    /// Returns an iterator that yields mutable references to all key-value pairs in the multimap by
    /// insertion order.
    ///
    /// Only the values are mutable, the keys are immutable.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key1", "value1");
    /// map.insert("key2", "value1");
    /// map.append(&"key1", "value2");
    /// map.append(&"key2", "value2");
    ///
    /// let mut iter = map.iter_mut();
    /// assert_eq!(iter.size_hint(), (4, Some(4)));
    ///
    /// let first = iter.next().unwrap();
    /// assert_eq!(first, (&"key1", &mut "value1"));
    /// *first.1 = "value3";
    ///
    /// assert_eq!(iter.next(), Some((&"key2", &mut "value1")));
    /// assert_eq!(iter.next(), Some((&"key1", &mut "value2")));
    /// assert_eq!(iter.next(), Some((&"key2", &mut "value2")));
    /// assert_eq!(iter.next(), None);
    ///
    /// assert_eq!(map.get(&"key1"), Some(&"value3"));
    /// ```
    pub fn iter_mut(&mut self) -> IterMut<Key, Value> {
        IterMut {
            keys: &self.keys,
            iter: self.values.iter_mut(),
        }
    }

    /// Computes the hash value of the given key.
    fn key_hash<KeyQuery>(&self, key: &KeyQuery) -> KeyHash
    where
        Key: Borrow<KeyQuery>,
        KeyQuery: ?Sized + Eq + Hash,
    {
        let mut hasher = self.hasher().build_hasher();
        key.borrow().hash(&mut hasher);
        KeyHash(hasher.finish())
    }

    /// Returns an iterator that yields immutable references to all keys in the multimap by
    /// insertion order.
    ///
    /// Insertion order of keys is determined by the order in which a given key is first inserted
    /// into the multimap with a value. Any subsequent insertions with that key without first
    /// removing it will not affect its ordering.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key1", "value");
    /// map.insert("key2", "value");
    /// map.insert("key3", "value");
    ///
    /// let mut keys = map.keys();
    /// assert_eq!(keys.next(), Some(&"key1"));
    /// assert_eq!(keys.next(), Some(&"key2"));
    /// assert_eq!(keys.next(), Some(&"key3"));
    /// assert_eq!(keys.next(), None);
    /// ```
    pub fn keys(&self) -> Keys<Key> {
        Keys(self.keys.iter())
    }

    /// Returns the number of keys the multimap can hold without reallocating.
    ///
    /// This number is a lower bound, and the multimap may be able to hold more.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// assert_eq!(map.keys_capacity(), 0);
    ///
    /// map.insert("key", "value");
    /// assert!(map.keys_capacity() > 0);
    /// ```
    pub fn keys_capacity(&self) -> usize {
        self.keys.capacity()
    }

    /// Returns the number of keys in the multimap.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// assert_eq!(map.keys_len(), 0);
    ///
    /// map.insert("key1", "value");
    /// map.insert("key2", "value");
    /// map.insert("key3", "value");
    /// assert_eq!(map.keys_len(), 3);
    /// ```
    pub fn keys_len(&self) -> usize {
        self.keys.len()
    }

    /// Reorganizes the multimap to ensure maximum spatial locality and changes the key and value
    /// capacities to the provided values.
    ///
    /// This function can be used to actually increase the capacity of the multimap.
    ///
    /// Complexity: O(|K| + |V|) where |K| is the number of keys and |V| is the number of values.
    ///
    /// # Panics
    ///
    /// Panics if either of the given minimum capacities are less than their current respective
    /// lengths.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::with_capacity(10, 10);
    ///
    /// map.insert("key1", "value1");
    /// map.insert("key2", "value2");
    /// map.append("key2", "value3");
    /// map.append("key1", "value4");
    /// map.pack_to(5, 5);
    ///
    /// assert_eq!(map.keys_capacity(), 5);
    /// assert_eq!(map.keys_len(), 2);
    /// assert_eq!(map.values_capacity(), 5);
    /// assert_eq!(map.values_len(), 4);
    /// ```
    pub fn pack_to(&mut self, keys_minimum_capacity: usize, values_minimum_capacity: usize)
    where
        State: Default,
    {
        assert!(
            keys_minimum_capacity >= self.keys_len(),
            "cannot pack multimap keys lower than current length"
        );
        assert!(
            values_minimum_capacity >= self.values_len(),
            "cannot pack multimap values lower than current length"
        );

        let key_map = self.keys.pack_to(keys_minimum_capacity);
        let value_map = self.values.pack_to(values_minimum_capacity);
        let mut map = HashMap::with_capacity_and_hasher(keys_minimum_capacity, State::default());

        for value_entry in self.values.iter_mut() {
            value_entry.key_index = *key_map.get(&value_entry.key_index).unwrap();
            value_entry.next_index = value_entry
                .next_index
                .map(|index| *value_map.get(&index).unwrap());
            value_entry.previous_index = value_entry
                .previous_index
                .map(|index| *value_map.get(&index).unwrap());
        }

        for (key, mut map_entry) in self.map.drain() {
            map_entry.head_index = *value_map.get(&map_entry.head_index).unwrap();
            map_entry.key_index = *key_map.get(&map_entry.key_index).unwrap();
            map_entry.tail_index = *value_map.get(&map_entry.tail_index).unwrap();
            map.insert(key, map_entry);
        }
    }

    /// Reorganizes the multimap to ensure maximum spatial locality and removes any excess key and
    /// value capacity.
    ///
    /// Complexity: O(|K| + |V|) where |K| is the number of keys and |V| is the number of values.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::with_capacity(5, 5);
    ///
    /// map.insert("key1", "value1");
    /// map.insert("key2", "value2");
    /// map.append("key2", "value3");
    /// map.append("key1", "value4");
    /// map.pack_to_fit();
    ///
    /// assert_eq!(map.keys_capacity(), 2);
    /// assert_eq!(map.keys_len(), 2);
    /// assert_eq!(map.values_capacity(), 4);
    /// assert_eq!(map.values_len(), 4);
    /// ```
    pub fn pack_to_fit(&mut self)
    where
        State: Default,
    {
        self.pack_to(self.keys_len(), self.values_len());
    }

    /// Removes all values associated with the given key from the map and returns the first value
    /// by insertion order.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    ///
    /// let removed_value = map.remove(&"key");
    /// assert_eq!(removed_value, None);
    ///
    /// map.insert("key", "value");
    /// assert_eq!(map.get(&"key"), Some(&"value"));
    ///
    /// let removed_value = map.remove(&"key");
    /// assert_eq!(removed_value, Some("value"));
    /// assert_eq!(map.get(&"key"), None);
    /// ```
    pub fn remove<KeyQuery>(&mut self, key: &KeyQuery) -> Option<Value>
    where
        Key: Borrow<KeyQuery>,
        KeyQuery: ?Sized + Eq + Hash,
    {
        self.remove_all(key).next()
    }

    /// Removes all values associated with the given key from the map and returns an iterator that
    /// yields those values.
    ///
    /// If the key is not already in the map, the iterator will yield no values.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    ///
    /// {
    ///     let mut removed_values = map.remove_all(&"key");
    ///     assert_eq!(removed_values.next(), None);
    /// }
    ///
    /// map.insert("key", "value1");
    /// map.append("key", "value2");
    /// assert_eq!(map.get(&"key"), Some(&"value1"));
    ///
    /// {
    ///     let mut removed_values = map.remove_all(&"key");
    ///     assert_eq!(removed_values.next(), Some("value1"));
    ///     assert_eq!(removed_values.next(), Some("value2"));
    ///     assert_eq!(removed_values.next(), None);
    /// }
    ///
    /// assert_eq!(map.get(&"key"), None);
    /// ```
    pub fn remove_all<KeyQuery>(&mut self, key: &KeyQuery) -> EntryValuesDrain<Key, Value>
    where
        Key: Borrow<KeyQuery>,
        KeyQuery: ?Sized + Eq + Hash,
    {
        match self.map.remove(&self.key_hash(key)) {
            Some(map_entry) => {
                self.keys.remove(map_entry.key_index).unwrap();
                EntryValuesDrain::from_map_entry(&mut self.values, &map_entry)
            }
            None => EntryValuesDrain::empty(&mut self.values),
        }
    }

    /// Reserves additional capacity such that more keys can be stored in the multimap.
    ///
    /// If the existing capacity minus the current length is enough to satisfy the additional
    /// capacity, the capacity will remain unchanged.
    ///
    /// If the capacity is increased, the capacity may be increased by more than what was requested.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::with_capacity(1, 1);
    ///
    /// map.insert("key", "value");
    /// assert_eq!(map.keys_capacity(), 1);
    ///
    /// map.reserve_keys(10);
    /// assert!(map.keys_capacity() >= 11);
    /// ```
    pub fn reserve_keys(&mut self, additional_capacity: usize) {
        self.keys.reserve(additional_capacity);
        self.map.reserve(additional_capacity);
    }

    /// Reserves additional capacity such that more values can be stored in the multimap.
    ///
    /// If the existing capacity minus the current length is enough to satisfy the additional
    /// capacity, the capacity will remain unchanged.
    ///
    /// If the capacity is increased, the capacity may be increased by more than what was requested.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::with_capacity(1, 1);
    ///
    /// map.insert("key", "value");
    /// assert_eq!(map.values_capacity(), 1);
    ///
    /// map.reserve_values(10);
    /// assert!(map.values_capacity() >= 11);
    /// ```
    pub fn reserve_values(&mut self, additional_capacity: usize) {
        self.values.reserve(additional_capacity);
    }

    /// Returns an iterator that yields immutable references to all values in the multimap by
    /// insertion order.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key1", "value1");
    /// map.insert("key2", "value1");
    /// map.append(&"key1", "value2");
    /// map.append(&"key2", "value2");
    ///
    /// let mut iter = map.values();
    /// assert_eq!(iter.size_hint(), (4, Some(4)));
    /// assert_eq!(iter.next(), Some(&"value1"));
    /// assert_eq!(iter.next(), Some(&"value1"));
    /// assert_eq!(iter.next(), Some(&"value2"));
    /// assert_eq!(iter.next(), Some(&"value2"));
    /// assert_eq!(iter.next(), None);
    /// ```
    pub fn values(&self) -> Values<Key, Value> {
        Values(self.values.iter())
    }

    /// Returns an iterator that yields mutable references to all values in the multimap by
    /// insertion order.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key1", "value1");
    /// map.insert("key2", "value1");
    /// map.append(&"key1", "value2");
    /// map.append(&"key2", "value2");
    ///
    /// let mut iter = map.values_mut();
    /// assert_eq!(iter.size_hint(), (4, Some(4)));
    ///
    /// let first = iter.next().unwrap();
    /// assert_eq!(first, &mut "value1");
    /// *first = "value3";
    ///
    /// assert_eq!(iter.next(), Some(&mut "value1"));
    /// assert_eq!(iter.next(), Some(&mut "value2"));
    /// assert_eq!(iter.next(), Some(&mut "value2"));
    /// assert_eq!(iter.next(), None);
    ///
    /// assert_eq!(map.get(&"key1"), Some(&"value3"));
    /// ```
    pub fn values_mut(&mut self) -> ValuesMut<Key, Value> {
        ValuesMut(self.values.iter_mut())
    }

    /// Returns the number of values the multimap can hold without reallocating.
    ///
    /// This number is a lower bound, and the multimap may be able to hold more.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// assert_eq!(map.values_capacity(), 0);
    ///
    /// map.insert("key", "value");
    /// assert!(map.values_capacity() > 0);
    /// ```
    pub fn values_capacity(&self) -> usize {
        self.values.capacity()
    }

    /// Returns the total number of values in the multimap across all keys.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// assert_eq!(map.values_len(), 0);
    ///
    /// map.insert("key1", "value1");
    /// assert_eq!(map.values_len(), 1);
    ///
    /// map.append("key1", "value2");
    /// assert_eq!(map.values_len(), 2);
    /// ```
    pub fn values_len(&self) -> usize {
        self.values.len()
    }

    /// Creates a new multimap with the specified capacities and the given hash builder to hash
    /// keys.
    ///
    /// The multimap will be able to hold at least `key_capacity` keys and `value_capacity` values
    /// without reallocating. A capacity of 0 will result in no allocation for the respective
    /// container.
    ///
    /// The `state` is normally randomly generated and is designed to allow multimaps to be
    /// resistant to attacks that cause many collisions and very poor performance. Setting it
    /// manually using this function can expose a DoS attack vector.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let state = RandomState::new();
    /// let mut map = ListOrderedMultimap::with_capacity_and_hasher(10, 10, state);
    /// map.insert("key", "value");
    /// assert_eq!(map.keys_capacity(), 10);
    /// assert_eq!(map.values_capacity(), 10);
    /// ```
    pub fn with_capacity_and_hasher(
        key_capacity: usize,
        value_capacity: usize,
        state: State,
    ) -> ListOrderedMultimap<Key, Value, State> {
        ListOrderedMultimap {
            keys: VecList::with_capacity(key_capacity),
            map: HashMap::with_capacity_and_hasher(key_capacity, state),
            values: VecList::with_capacity(value_capacity),
        }
    }

    /// Creates a new multimap with no capacity which will use the given hash builder to hash keys.
    ///
    /// The `state` is normally randomly generated and is designed to allow multimaps to be
    /// resistant to attacks that cause many collisions and very poor performance. Setting it
    /// manually using this function can expose a DoS attack vector.
    ///
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let state = RandomState::new();
    /// let mut map = ListOrderedMultimap::with_hasher(state);
    /// map.insert("key", "value");
    /// ```
    pub fn with_hasher(state: State) -> ListOrderedMultimap<Key, Value, State> {
        ListOrderedMultimap {
            keys: VecList::new(),
            map: HashMap::with_hasher(state),
            values: VecList::new(),
        }
    }
}

impl<Key, Value, State> Debug for ListOrderedMultimap<Key, Value, State>
where
    Key: Debug + Eq + Hash,
    Value: Debug,
    State: BuildHasher,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.debug_map().entries(self.iter()).finish()
    }
}

impl<Key, Value> Default for ListOrderedMultimap<Key, Value, RandomState>
where
    Key: Eq + Hash,
{
    fn default() -> Self {
        ListOrderedMultimap {
            keys: VecList::new(),
            map: HashMap::with_hasher(RandomState::new()),
            values: VecList::new(),
        }
    }
}

impl<Key, Value, State> Eq for ListOrderedMultimap<Key, Value, State>
where
    Key: Eq + Hash,
    Value: PartialEq,
    State: BuildHasher,
{}

impl<Key, Value, State> Extend<(Key, Value)> for ListOrderedMultimap<Key, Value, State>
where
    Key: Eq + Hash,
    State: BuildHasher,
{
    fn extend<Iter>(&mut self, iter: Iter)
    where
        Iter: IntoIterator<Item = (Key, Value)>,
    {
        let iter = iter.into_iter();
        self.reserve_values(iter.size_hint().0);

        for (key, value) in iter {
            self.append(key, value);
        }
    }
}

impl<'a, Key, Value, State> Extend<(&'a Key, &'a Value)> for ListOrderedMultimap<Key, Value, State>
where
    Key: Copy + Eq + Hash,
    Value: Copy,
    State: BuildHasher,
{
    fn extend<Iter>(&mut self, iter: Iter)
    where
        Iter: IntoIterator<Item = (&'a Key, &'a Value)>,
    {
        self.extend(iter.into_iter().map(|(&key, &value)| (key, value)));
    }
}

impl<Key, Value, State> FromIterator<(Key, Value)> for ListOrderedMultimap<Key, Value, State>
where
    Key: Eq + Hash,
    State: BuildHasher + Default,
{
    fn from_iter<Iter>(iter: Iter) -> Self
    where
        Iter: IntoIterator<Item = (Key, Value)>,
    {
        let mut map = ListOrderedMultimap::with_hasher(State::default());
        map.extend(iter);
        map
    }
}

impl<'map, Key, Value, State> IntoIterator for &'map ListOrderedMultimap<Key, Value, State>
where
    Key: Eq + Hash,
    State: BuildHasher,
{
    type IntoIter = Iter<'map, Key, Value>;
    type Item = (&'map Key, &'map Value);

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'map, Key, Value, State> IntoIterator for &'map mut ListOrderedMultimap<Key, Value, State>
where
    Key: Eq + Hash,
    State: BuildHasher,
{
    type IntoIter = IterMut<'map, Key, Value>;
    type Item = (&'map Key, &'map mut Value);

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<Key, Value, State> IntoIterator for ListOrderedMultimap<Key, Value, State>
where
    Key: Eq + Hash,
    State: BuildHasher,
{
    type IntoIter = IntoIter<Key, Value>;
    type Item = (Key, Value);

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            keys: self.keys,
            iter: self.values.into_iter(),
        }
    }
}

impl<Key, Value, State> PartialEq for ListOrderedMultimap<Key, Value, State>
where
    Key: Eq + Hash,
    Value: PartialEq,
    State: BuildHasher,
{
    fn eq(&self, other: &ListOrderedMultimap<Key, Value, State>) -> bool {
        if self.keys_len() != other.keys_len() || self.values_len() != other.values_len() {
            return false;
        }

        self.iter().eq(other.iter())
    }
}

/// The hash value of a key. This is used to avoid having to store the actual key in the internal
/// hash map.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct KeyHash(u64);

/// The value type of the internal hash map.
#[derive(Clone)]
struct MapEntry<Key, Value> {
    head_index: Index<ValueEntry<Key, Value>>,

    /// The index of the key in the key list for this entry.
    key_index: Index<Key>,

    length: usize,

    tail_index: Index<ValueEntry<Key, Value>>,
}

impl<Key, Value> MapEntry<Key, Value> {
    pub fn append(&mut self, index: Index<ValueEntry<Key, Value>>) {
        self.length += 1;
        self.tail_index = index;
    }

    /// Convenience function for creating a new multimap entry.
    pub fn new(key_index: Index<Key>, index: Index<ValueEntry<Key, Value>>) -> Self {
        MapEntry {
            head_index: index,
            key_index,
            length: 1,
            tail_index: index,
        }
    }

    pub fn reset(&mut self, index: Index<ValueEntry<Key, Value>>) {
        self.head_index = index;
        self.length = 1;
        self.tail_index = index;
    }
}

/// The value entry that is contained within the internal values list.
#[derive(Clone)]
struct ValueEntry<Key, Value> {
    /// The index of the key in the key list for this entry.
    key_index: Index<Key>,

    next_index: Option<Index<ValueEntry<Key, Value>>>,

    previous_index: Option<Index<ValueEntry<Key, Value>>>,

    /// The actual value stored in this entry.
    value: Value,
}

impl<Key, Value> ValueEntry<Key, Value> {
    /// Convenience function for creating a new value entry.
    pub fn new(key_index: Index<Key>, value: Value) -> Self {
        ValueEntry {
            key_index,
            next_index: None,
            previous_index: None,
            value,
        }
    }
}

/// A view into a single entry in the multimap, which may either be vacant or occupied.
pub enum Entry<'map, Key, Value, State = RandomState> {
    /// An occupied entry associated with one or more values.
    Occupied(OccupiedEntry<'map, Key, Value, State>),

    /// A vacant entry with no associated values.
    Vacant(VacantEntry<'map, Key, Value, State>),
}

impl<'map, Key, Value, State> Entry<'map, Key, Value, State>
where
    State: BuildHasher,
{
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    ///
    /// map.entry("key")
    ///     .and_modify(|value| *value += 1)
    ///     .or_insert(42);
    /// assert_eq!(map.get(&"key"), Some(&42));
    ///
    /// map.entry("key")
    ///     .and_modify(|value| *value += 1)
    ///     .or_insert(42);
    /// assert_eq!(map.get(&"key"), Some(&43));
    /// ```
    pub fn and_modify<Function>(self, function: Function) -> Self
    where
        Function: FnOnce(&mut Value),
    {
        use self::Entry::*;

        match self {
            Occupied(mut entry) => {
                function(entry.get_mut());
                Occupied(entry)
            }
            Vacant(entry) => Vacant(entry),
        }
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let value = map.entry("key").or_insert("value2");
    /// assert_eq!(value, &"value1");
    ///
    /// let value = map.entry("key2").or_insert("value2");
    /// assert_eq!(value, &"value2");
    /// ```
    pub fn or_insert(self, value: Value) -> &'map mut Value {
        use self::Entry::*;

        match self {
            Occupied(entry) => entry.into_mut(),
            Vacant(entry) => entry.insert(value),
        }
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let entry = map.entry("key").or_insert_entry("value2");
    /// assert_eq!(entry.into_mut(), &"value1");
    ///
    /// let entry = map.entry("key2").or_insert_entry("value2");
    /// assert_eq!(entry.into_mut(), &"value2");
    /// ```
    pub fn or_insert_entry(self, value: Value) -> OccupiedEntry<'map, Key, Value, State> {
        use self::Entry::*;

        match self {
            Occupied(entry) => entry,
            Vacant(entry) => entry.insert_entry(value),
        }
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let value = map.entry("key").or_insert_with(|| "value2");
    /// assert_eq!(value, &"value1");
    ///
    /// let value = map.entry("key2").or_insert_with(|| "value2");
    /// assert_eq!(value, &"value2");
    /// ```
    pub fn or_insert_with<Function>(self, function: Function) -> &'map mut Value
    where
        Function: FnOnce() -> Value,
    {
        use self::Entry::*;

        match self {
            Occupied(entry) => entry.into_mut(),
            Vacant(entry) => entry.insert(function()),
        }
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::ListOrderedMultimap;
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let entry = map.entry("key").or_insert_with_entry(|| "value2");
    /// assert_eq!(entry.into_mut(), &"value1");
    ///
    /// let entry = map.entry("key2").or_insert_with_entry(|| "value2");
    /// assert_eq!(entry.into_mut(), &"value2");
    /// ```
    pub fn or_insert_with_entry<Function>(
        self,
        function: Function,
    ) -> OccupiedEntry<'map, Key, Value, State>
    where
        Function: FnOnce() -> Value,
    {
        use self::Entry::*;

        match self {
            Occupied(entry) => entry,
            Vacant(entry) => entry.insert_entry(function()),
        }
    }
}

/// A view into an occupied entry in the multimap.
pub struct OccupiedEntry<'map, Key, Value, State = RandomState> {
    /// The hash of the key for the entry.
    hash: KeyHash,

    /// Reference to the multimap.
    map: &'map mut ListOrderedMultimap<Key, Value, State>,
}

impl<'map, Key, Value, State> OccupiedEntry<'map, Key, Value, State>
where
    State: BuildHasher,
{
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// entry.append("value2");
    ///
    /// let mut iter = map.get_all(&"key");
    /// assert_eq!(iter.next(), Some(&"value1"));
    /// assert_eq!(iter.next(), Some(&"value2"));
    /// assert_eq!(iter.next(), None);
    /// ```
    pub fn append(&mut self, value: Value) {
        let map_entry = self.map.map.get_mut(&self.hash).unwrap();
        let value_entry = ValueEntry::new(map_entry.key_index, value);
        let index = self.map.values.push_back(value_entry);
        self.map
            .values
            .get_mut(map_entry.tail_index)
            .unwrap()
            .next_index = Some(index);
        map_entry.length += 1;
        map_entry.tail_index = index;
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// assert_eq!(entry.get(), &"value");
    /// ```
    pub fn get(&self) -> &Value {
        let index = self.map.map.get(&self.hash).unwrap().head_index;
        &self.map.values.get(index).unwrap().value
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// assert_eq!(entry.get(), &mut "value");
    /// ```
    pub fn get_mut(&mut self) -> &mut Value {
        let index = self.map.map.get(&self.hash).unwrap().head_index;
        &mut self.map.values.get_mut(index).unwrap().value
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// entry.insert("value2");
    ///
    /// assert_eq!(map.get(&"key"), Some(&"value2"));
    /// ```
    pub fn insert(&mut self, value: Value) -> Value {
        let map_entry = self.map.map.get_mut(&self.hash).unwrap();
        let first_index = map_entry.head_index;
        let mut entry = self.map.values.remove(first_index).unwrap();
        let first_value = entry.value;

        while let Some(next_index) = entry.next_index {
            entry = self.map.values.remove(next_index).unwrap();
        }

        let value_entry = ValueEntry::new(map_entry.key_index, value);
        let index = self.map.values.push_back(value_entry);
        map_entry.head_index = index;
        map_entry.length = 1;
        map_entry.tail_index = index;
        first_value
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// entry.append("value2");
    ///
    /// let mut iter = entry.insert_all("value3");
    /// assert_eq!(iter.next(), Some("value1"));
    /// assert_eq!(iter.next(), Some("value2"));
    /// assert_eq!(iter.next(), None);
    /// ```
    pub fn insert_all(&mut self, value: Value) -> EntryValuesDrain<Key, Value> {
        let map_entry = self.map.map.get_mut(&self.hash).unwrap();
        let value_entry = ValueEntry::new(map_entry.key_index, value);
        let index = self.map.values.push_back(value_entry);
        let iter = EntryValuesDrain::from_map_entry(&mut self.map.values, &map_entry);
        map_entry.reset(index);
        iter
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// assert_eq!(entry.into_mut(), &mut "value");
    /// ```
    pub fn into_mut(self) -> &'map mut Value {
        let index = self.map.map.get_mut(&self.hash).unwrap().head_index;
        &mut self.map.values.get_mut(index).unwrap().value
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// entry.append("value2");
    ///
    /// let mut iter = entry.iter();
    /// assert_eq!(iter.next(), Some(&"value1"));
    /// assert_eq!(iter.next(), Some(&"value2"));
    /// assert_eq!(iter.next(), None);
    /// ```
    pub fn iter(&self) -> EntryValues<Key, Value> {
        let map_entry = self.map.map.get(&self.hash).unwrap();
        EntryValues::from_map_entry(&self.map.values, &map_entry)
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// entry.append("value2");
    ///
    /// let mut iter = entry.iter_mut();
    /// assert_eq!(iter.next(), Some(&mut "value1"));
    /// assert_eq!(iter.next(), Some(&mut "value2"));
    /// assert_eq!(iter.next(), None);
    /// ```
    pub fn iter_mut(&mut self) -> EntryValuesMut<Key, Value> {
        let map_entry = self.map.map.get_mut(&self.hash).unwrap();
        EntryValuesMut::from_map_entry(&mut self.map.values, &map_entry)
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// assert_eq!(entry.key(), &"key");
    /// ```
    pub fn key(&self) -> &Key {
        let key_index = self.map.map.get(&self.hash).unwrap().key_index;
        self.map.keys.get(key_index).unwrap()
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// assert_eq!(entry.len(), 1);
    ///
    /// entry.append("value2");
    /// assert_eq!(entry.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        self.map.map.get(&self.hash).unwrap().length
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// assert_eq!(entry.remove(), "value");
    /// ```
    pub fn remove(self) -> Value {
        let map_entry = self.map.map.remove(&self.hash).unwrap();
        self.map.keys.remove(map_entry.key_index).unwrap();
        let first_index = map_entry.head_index;
        let mut entry = self.map.values.remove(first_index).unwrap();
        let first_value = entry.value;

        while let Some(next_index) = entry.next_index {
            entry = self.map.values.remove(next_index).unwrap();
        }

        first_value
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// entry.append("value2");
    ///
    /// let mut iter = entry.remove_all();
    /// assert_eq!(iter.next(), Some("value1"));
    /// assert_eq!(iter.next(), Some("value2"));
    /// assert_eq!(iter.next(), None);
    /// ```
    pub fn remove_all(self) -> EntryValuesDrain<'map, Key, Value> {
        self.remove_entry_all().1
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// assert_eq!(entry.remove_entry(), ("key", "value"));
    /// ```
    pub fn remove_entry(self) -> (Key, Value) {
        let map_entry = self.map.map.remove(&self.hash).unwrap();
        let key = self.map.keys.remove(map_entry.key_index).unwrap();
        let first_index = map_entry.head_index;
        let mut entry = self.map.values.remove(first_index).unwrap();
        let first_value = entry.value;

        while let Some(next_index) = entry.next_index {
            entry = self.map.values.remove(next_index).unwrap();
        }

        (key, first_value)
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    /// map.insert("key", "value1");
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Occupied(entry) => entry,
    ///     _ => panic!("expected occupied entry")
    /// };
    ///
    /// entry.append("value2");
    ///
    /// let (key, mut iter) = entry.remove_entry_all();
    /// assert_eq!(key, "key");
    /// assert_eq!(iter.next(), Some("value1"));
    /// assert_eq!(iter.next(), Some("value2"));
    /// assert_eq!(iter.next(), None);
    /// ```
    pub fn remove_entry_all(self) -> (Key, EntryValuesDrain<'map, Key, Value>) {
        let map_entry = self.map.map.remove(&self.hash).unwrap();
        let key = self.map.keys.remove(map_entry.key_index).unwrap();
        let iter = EntryValuesDrain {
            head_index: Some(map_entry.head_index),
            remaining: map_entry.length,
            tail_index: Some(map_entry.tail_index),
            values: &mut self.map.values,
        };
        (key, iter)
    }
}

impl<Key, Value, State> Debug for OccupiedEntry<'_, Key, Value, State>
where
    Key: Debug,
    Value: Debug,
    State: BuildHasher,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter
            .debug_struct("OccupiedEntry")
            .field("key", self.key())
            .field("value", self.get())
            .finish()
    }
}

/// A view into a vacant entry in the multimap.
pub struct VacantEntry<'map, Key, Value, State = RandomState> {
    /// The hash of the key for the entry.
    hash: KeyHash,

    /// The key for this entry for when it is to be inserted into the map.
    key: Key,

    /// Reference to the multimap.
    map: &'map mut ListOrderedMultimap<Key, Value, State>,
}

impl<'map, Key, Value, State> VacantEntry<'map, Key, Value, State>
where
    State: BuildHasher,
{
    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Vacant(entry) => entry,
    ///     _ => panic!("expected vacant entry")
    /// };
    ///
    /// assert_eq!(entry.insert("value"), &"value");
    /// ```
    pub fn insert(self, value: Value) -> &'map mut Value {
        let key_index = self.map.keys.push_back(self.key);
        let value_entry = ValueEntry::new(key_index, value);
        let index = self.map.values.push_back(value_entry);
        let map_entry = MapEntry::new(key_index, index);
        self.map.map.insert(self.hash, map_entry);
        &mut self.map.values.get_mut(index).unwrap().value
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map = ListOrderedMultimap::new();
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Vacant(entry) => entry,
    ///     _ => panic!("expected vacant entry")
    /// };
    ///
    /// let mut entry = entry.insert_entry("value");
    /// assert_eq!(entry.get(), &"value");
    /// ```
    pub fn insert_entry(self, value: Value) -> OccupiedEntry<'map, Key, Value, State> {
        let key_index = self.map.keys.push_back(self.key);
        let value_entry = ValueEntry::new(key_index, value);
        let index = self.map.values.push_back(value_entry);
        let map_entry = MapEntry::new(key_index, index);
        self.map.map.insert(self.hash, map_entry);

        OccupiedEntry {
            hash: self.hash,
            map: self.map,
        }
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::new();
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Vacant(entry) => entry,
    ///     _ => panic!("expected vacant entry")
    /// };
    ///
    /// assert_eq!(entry.into_key(), "key");
    /// ```
    pub fn into_key(self) -> Key {
        self.key
    }

    /// # Examples
    ///
    /// ```
    /// use ordered_multimap::{Entry, ListOrderedMultimap};
    ///
    /// let mut map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::new();
    ///
    /// let mut entry = match map.entry("key") {
    ///     Entry::Vacant(entry) => entry,
    ///     _ => panic!("expected vacant entry")
    /// };
    ///
    /// assert_eq!(entry.key(), &"key");
    /// ```
    pub fn key(&self) -> &Key {
        &self.key
    }
}

impl<Key, Value, State> Debug for VacantEntry<'_, Key, Value, State>
where
    Key: Debug,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter
            .debug_tuple("VacantEntry")
            .field(&self.key)
            .finish()
    }
}

pub struct Drain<'map, Key, Value, State = RandomState>
where
    State: BuildHasher,
{
    iter: VecListDrain<'map, ValueEntry<Key, Value>>,

    keys: &'map mut VecList<Key>,

    map: &'map mut HashMap<KeyHash, MapEntry<Key, Value>, State>,
}

impl<Key, Value, State> Drain<'_, Key, Value, State>
where
    Key: Eq + Hash,
    State: BuildHasher,
{
    fn key_hash(&self, key: &Key) -> KeyHash {
        let mut hasher = self.map.hasher().build_hasher();
        key.hash(&mut hasher);
        KeyHash(hasher.finish())
    }

    pub fn iter(&self) -> Iter<Key, Value> {
        Iter {
            keys: &self.keys,
            iter: self.iter.iter(),
        }
    }
}

impl<Key, Value, State> Debug for Drain<'_, Key, Value, State>
where
    Key: Debug + Eq + Hash,
    State: BuildHasher,
    Value: Debug,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("Drain(")?;
        formatter.debug_list().entries(self.iter()).finish()?;
        formatter.write_str(")")
    }
}

impl<Key, Value, State> DoubleEndedIterator for Drain<'_, Key, Value, State>
where
    Key: Eq + Hash,
    State: BuildHasher,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let value_entry = self.iter.next_back()?;
        let key = self.keys.remove(value_entry.key_index).unwrap();
        let hash = self.key_hash(&key);
        self.map.remove(&hash);
        Some((key, value_entry.value))
    }
}

impl<Key, Value, State> ExactSizeIterator for Drain<'_, Key, Value, State>
where
    Key: Eq + Hash,
    State: BuildHasher,
{}

impl<Key, Value, State> FusedIterator for Drain<'_, Key, Value, State>
where
    Key: Eq + Hash,
    State: BuildHasher,
{}

impl<Key, Value, State> Iterator for Drain<'_, Key, Value, State>
where
    Key: Eq + Hash,
    State: BuildHasher,
{
    type Item = (Key, Value);

    fn next(&mut self) -> Option<Self::Item> {
        let value_entry = self.iter.next()?;
        let key = self.keys.remove(value_entry.key_index).unwrap();
        let hash = self.key_hash(&key);
        self.map.remove(&hash);
        Some((key, value_entry.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// An iterator that yields immutable references to all values of a given key. The order of the
/// values is always in the order that they were inserted.
pub struct EntryValues<'map, Key, Value> {
    head_index: Option<Index<ValueEntry<Key, Value>>>,

    remaining: usize,

    tail_index: Option<Index<ValueEntry<Key, Value>>>,

    /// The list of the values in the map. This is ordered by time of insertion.
    values: &'map VecList<ValueEntry<Key, Value>>,
}

impl<'map, Key, Value> EntryValues<'map, Key, Value> {
    fn empty(values: &'map VecList<ValueEntry<Key, Value>>) -> Self {
        EntryValues {
            head_index: None,
            remaining: 0,
            tail_index: None,
            values,
        }
    }

    fn from_map_entry(
        values: &'map VecList<ValueEntry<Key, Value>>,
        map_entry: &MapEntry<Key, Value>,
    ) -> Self {
        EntryValues {
            head_index: Some(map_entry.head_index),
            remaining: map_entry.length,
            tail_index: Some(map_entry.tail_index),
            values,
        }
    }
}

impl<'map, Key, Value> Clone for EntryValues<'map, Key, Value> {
    fn clone(&self) -> EntryValues<'map, Key, Value> {
        EntryValues {
            head_index: self.head_index,
            remaining: self.remaining,
            tail_index: self.tail_index,
            values: self.values,
        }
    }
}

impl<Key, Value> Debug for EntryValues<'_, Key, Value>
where
    Value: Debug,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("EntryValues(")?;
        formatter.debug_list().entries(self.clone()).finish()?;
        formatter.write_str(")")
    }
}

impl<Key, Value> DoubleEndedIterator for EntryValues<'_, Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            self.head_index.map(|index| {
                let entry = self.values.get(index).unwrap();
                self.tail_index = entry.previous_index;
                self.remaining -= 1;
                &entry.value
            })
        }
    }
}

impl<Key, Value> ExactSizeIterator for EntryValues<'_, Key, Value> {}

impl<Key, Value> FusedIterator for EntryValues<'_, Key, Value> {}

impl<'map, Key, Value> Iterator for EntryValues<'map, Key, Value> {
    type Item = &'map Value;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            self.head_index.map(|index| {
                let entry = self.values.get(index).unwrap();
                self.head_index = entry.next_index;
                self.remaining -= 1;
                &entry.value
            })
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

/// An iterator that moves all values of a given key out of a multimap but preserves the underlying
/// vector. The order of the values is always in the order that they were inserted.
pub struct EntryValuesDrain<'map, Key, Value> {
    head_index: Option<Index<ValueEntry<Key, Value>>>,

    remaining: usize,

    tail_index: Option<Index<ValueEntry<Key, Value>>>,

    /// The list of the values in the map. This is ordered by time of insertion.
    values: &'map mut VecList<ValueEntry<Key, Value>>,
}

impl<'map, Key, Value> EntryValuesDrain<'map, Key, Value> {
    fn empty(values: &'map mut VecList<ValueEntry<Key, Value>>) -> Self {
        EntryValuesDrain {
            head_index: None,
            remaining: 0,
            tail_index: None,
            values,
        }
    }

    fn from_map_entry(
        values: &'map mut VecList<ValueEntry<Key, Value>>,
        map_entry: &MapEntry<Key, Value>,
    ) -> Self {
        EntryValuesDrain {
            head_index: Some(map_entry.head_index),
            remaining: map_entry.length,
            tail_index: Some(map_entry.tail_index),
            values,
        }
    }

    /// Creates an iterator that yields immutable references to all values of a given key.
    pub fn iter(&self) -> EntryValues<Key, Value> {
        EntryValues {
            head_index: self.head_index,
            remaining: self.remaining,
            tail_index: self.tail_index,
            values: self.values,
        }
    }
}

impl<Key, Value> Debug for EntryValuesDrain<'_, Key, Value>
where
    Key: Debug,
    Value: Debug,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("EntryValuesDrain(")?;
        formatter.debug_list().entries(self.iter()).finish()?;
        formatter.write_str(")")
    }
}

impl<Key, Value> DoubleEndedIterator for EntryValuesDrain<'_, Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            self.head_index.map(|index| {
                let entry = self.values.remove(index).unwrap();
                self.tail_index = entry.previous_index;
                self.remaining -= 1;
                entry.value
            })
        }
    }
}

impl<Key, Value> Drop for EntryValuesDrain<'_, Key, Value> {
    fn drop(&mut self) {
        for _ in self {}
    }
}

impl<Key, Value> ExactSizeIterator for EntryValuesDrain<'_, Key, Value> {}

impl<Key, Value> FusedIterator for EntryValuesDrain<'_, Key, Value> {}

impl<Key, Value> Iterator for EntryValuesDrain<'_, Key, Value> {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            self.head_index.map(|index| {
                let entry = self.values.remove(index).unwrap();
                self.head_index = entry.next_index;
                self.remaining -= 1;
                entry.value
            })
        }
    }
}

/// An iterator that yields mutable references to all values of a given key. The order of the values
/// is always in the order that they were inserted.
pub struct EntryValuesMut<'map, Key, Value> {
    head_index: Option<Index<ValueEntry<Key, Value>>>,

    phantom: PhantomData<&'map mut VecList<ValueEntry<Key, Value>>>,

    remaining: usize,

    tail_index: Option<Index<ValueEntry<Key, Value>>>,

    /// The list of the values in the map. This is ordered by time of insertion.
    values: *mut VecList<ValueEntry<Key, Value>>,
}

impl<'map, Key, Value> EntryValuesMut<'map, Key, Value> {
    fn empty(values: &'map mut VecList<ValueEntry<Key, Value>>) -> Self {
        EntryValuesMut {
            head_index: None,
            phantom: PhantomData,
            remaining: 0,
            tail_index: None,
            values: values as *mut _,
        }
    }

    fn from_map_entry(
        values: &'map mut VecList<ValueEntry<Key, Value>>,
        map_entry: &MapEntry<Key, Value>,
    ) -> Self {
        EntryValuesMut {
            head_index: Some(map_entry.head_index),
            phantom: PhantomData,
            remaining: map_entry.length,
            tail_index: Some(map_entry.tail_index),
            values: values as *mut _,
        }
    }

    /// Creates an iterator that yields immutable references to all values of a given key.
    pub fn iter(&self) -> EntryValues<Key, Value> {
        EntryValues {
            head_index: self.head_index,
            remaining: self.remaining,
            tail_index: self.tail_index,
            values: unsafe { &*self.values },
        }
    }
}

impl<Key, Value> Debug for EntryValuesMut<'_, Key, Value>
where
    Value: Debug,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("EntryValuesMut(")?;
        formatter.debug_list().entries(self.iter()).finish()?;
        formatter.write_str(")")
    }
}

impl<Key, Value> DoubleEndedIterator for EntryValuesMut<'_, Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            self.head_index.map(|index| {
                let entry = unsafe { (*self.values).get_mut(index) }.unwrap();
                self.tail_index = entry.previous_index;
                self.remaining -= 1;
                &mut entry.value
            })
        }
    }
}

impl<Key, Value> ExactSizeIterator for EntryValuesMut<'_, Key, Value> {}

impl<Key, Value> FusedIterator for EntryValuesMut<'_, Key, Value> {}

impl<'map, Key, Value> Iterator for EntryValuesMut<'map, Key, Value> {
    type Item = &'map mut Value;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            self.head_index.map(|index| {
                let entry = unsafe { (*self.values).get_mut(index) }.unwrap();
                self.head_index = entry.next_index;
                self.remaining -= 1;
                &mut entry.value
            })
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

pub struct IntoIter<Key, Value> {
    /// The list of the keys in the map. This is ordered by time of insertion.
    keys: VecList<Key>,

    /// The iterator over the list of all value entries.
    iter: VecListIntoIter<ValueEntry<Key, Value>>,
}

impl<Key, Value> IntoIter<Key, Value> {
    pub fn iter(&self) -> Iter<Key, Value> {
        Iter {
            keys: &self.keys,
            iter: self.iter.iter(),
        }
    }
}

impl<Key, Value> Debug for IntoIter<Key, Value>
where
    Key: Debug,
    Value: Debug,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("IntoIter(")?;
        formatter.debug_list().entries(self.iter()).finish()?;
        formatter.write_str(")")
    }
}

impl<Key, Value> DoubleEndedIterator for IntoIter<Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let value_entry = self.iter.next_back()?;
        let key = self.keys.remove(value_entry.key_index).unwrap();
        Some((key, value_entry.value))
    }
}

impl<Key, Value> ExactSizeIterator for IntoIter<Key, Value> {}

impl<Key, Value> FusedIterator for IntoIter<Key, Value> {}

impl<Key, Value> Iterator for IntoIter<Key, Value> {
    type Item = (Key, Value);

    fn next(&mut self) -> Option<Self::Item> {
        let value_entry = self.iter.next()?;
        let key = self.keys.remove(value_entry.key_index).unwrap();
        Some((key, value_entry.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// An iterator that yields immutable references to all key-value pairs in a multimap. The order of
/// the yielded items is always in the order that they were inserted.
pub struct Iter<'map, Key, Value> {
    // The list of the keys in the map. This is ordered by time of insertion.
    keys: &'map VecList<Key>,

    /// The iterator over the list of all values. This is ordered by time of insertion.
    iter: VecListIter<'map, ValueEntry<Key, Value>>,
}

impl<'map, Key, Value> Clone for Iter<'map, Key, Value> {
    fn clone(&self) -> Iter<'map, Key, Value> {
        Iter {
            keys: self.keys,
            iter: self.iter.clone(),
        }
    }
}

impl<Key, Value> Debug for Iter<'_, Key, Value>
where
    Key: Debug,
    Value: Debug,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("Iter(")?;
        formatter.debug_list().entries(self.clone()).finish()?;
        formatter.write_str(")")
    }
}

impl<Key, Value> DoubleEndedIterator for Iter<'_, Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let value_entry = self.iter.next_back()?;
        let key = self.keys.get(value_entry.key_index).unwrap();
        Some((key, &value_entry.value))
    }
}

impl<Key, Value> ExactSizeIterator for Iter<'_, Key, Value> {}

impl<Key, Value> FusedIterator for Iter<'_, Key, Value> {}

impl<'map, Key, Value> Iterator for Iter<'map, Key, Value> {
    type Item = (&'map Key, &'map Value);

    fn next(&mut self) -> Option<Self::Item> {
        let value_entry = self.iter.next()?;
        let key = self.keys.get(value_entry.key_index).unwrap();
        Some((key, &value_entry.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// An iterator that yields mutable references to all key-value pairs in a multimap. The order of
/// the yielded items is always in the order that they were inserted.
pub struct IterMut<'map, Key, Value> {
    // The list of the keys in the map. This is ordered by time of insertion.
    keys: &'map VecList<Key>,

    /// The iterator over the list of all values. This is ordered by time of insertion.
    iter: VecListIterMut<'map, ValueEntry<Key, Value>>,
}

impl<Key, Value> IterMut<'_, Key, Value> {
    pub fn iter(&self) -> Iter<Key, Value> {
        Iter {
            keys: self.keys,
            iter: self.iter.iter(),
        }
    }
}

impl<Key, Value> Debug for IterMut<'_, Key, Value>
where
    Key: Debug,
    Value: Debug,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("IterMut(")?;
        formatter.debug_list().entries(self.iter()).finish()?;
        formatter.write_str(")")
    }
}

impl<Key, Value> DoubleEndedIterator for IterMut<'_, Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let value_entry = self.iter.next_back()?;
        let key = self.keys.get(value_entry.key_index).unwrap();
        Some((key, &mut value_entry.value))
    }
}

impl<Key, Value> ExactSizeIterator for IterMut<'_, Key, Value> {}

impl<Key, Value> FusedIterator for IterMut<'_, Key, Value> {}

impl<'map, Key, Value> Iterator for IterMut<'map, Key, Value> {
    type Item = (&'map Key, &'map mut Value);

    fn next(&mut self) -> Option<Self::Item> {
        let value_entry = self.iter.next()?;
        let key = self.keys.get(value_entry.key_index).unwrap();
        Some((key, &mut value_entry.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// An iterator that yields mutable references to values of a given key. The order of the keys is
/// always in the order that they were first inserted.
pub struct Keys<'map, Key>(VecListIter<'map, Key>);

impl<'map, Key> Clone for Keys<'map, Key> {
    fn clone(&self) -> Keys<'map, Key> {
        Keys(self.0.clone())
    }
}

impl<Key> Debug for Keys<'_, Key>
where
    Key: Debug,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("Keys(")?;
        formatter.debug_list().entries(self.clone()).finish()?;
        formatter.write_str(")")
    }
}

impl<Key> DoubleEndedIterator for Keys<'_, Key> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}

impl<Key> ExactSizeIterator for Keys<'_, Key> {}

impl<Key> FusedIterator for Keys<'_, Key> {}

impl<'map, Key> Iterator for Keys<'map, Key> {
    type Item = &'map Key;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

/// An iterator that yields immutable references to all values of a multimap. The order of the
/// values is always in the order that they were inserted.
pub struct Values<'map, Key, Value>(VecListIter<'map, ValueEntry<Key, Value>>);

impl<'map, Key, Value> Clone for Values<'map, Key, Value> {
    fn clone(&self) -> Values<'map, Key, Value> {
        Values(self.0.clone())
    }
}

impl<Key, Value> Debug for Values<'_, Key, Value>
where
    Key: Debug,
    Value: Debug,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("Values(")?;
        formatter.debug_list().entries(self.clone()).finish()?;
        formatter.write_str(")")
    }
}

impl<Key, Value> DoubleEndedIterator for Values<'_, Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|entry| &entry.value)
    }
}

impl<Key, Value> ExactSizeIterator for Values<'_, Key, Value> {}

impl<Key, Value> FusedIterator for Values<'_, Key, Value> {}

impl<'map, Key, Value> Iterator for Values<'map, Key, Value> {
    type Item = &'map Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|entry| &entry.value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

/// An iterator that yields mutable references to all values of a multimap. The order of the values
/// is always in the order that they were inserted.
pub struct ValuesMut<'map, Key, Value>(VecListIterMut<'map, ValueEntry<Key, Value>>);

impl<Key, Value> ValuesMut<'_, Key, Value> {
    /// Creates an iterator that yields immutable references to all values of a multimap.
    pub fn iter(&self) -> Values<Key, Value> {
        Values(self.0.iter())
    }
}

impl<Key, Value> Debug for ValuesMut<'_, Key, Value>
where
    Key: Debug,
    Value: Debug,
{
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter
            .debug_tuple("ValuesMut")
            .field(&self.iter())
            .finish()
    }
}

impl<Key, Value> DoubleEndedIterator for ValuesMut<'_, Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|entry| &mut entry.value)
    }
}

impl<Key, Value> ExactSizeIterator for ValuesMut<'_, Key, Value> {}

impl<Key, Value> FusedIterator for ValuesMut<'_, Key, Value> {}

impl<'map, Key, Value> Iterator for ValuesMut<'map, Key, Value> {
    type Item = &'map mut Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|entry| &mut entry.value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bounds() {
        fn check_bounds<Type: Send + Sync>() {}

        check_bounds::<ListOrderedMultimap<(), ()>>();
        check_bounds::<OccupiedEntry<'static, (), ()>>();
        check_bounds::<VacantEntry<'static, (), ()>>();
    }

    #[test]
    fn test_list_ordered_multimap_append() {
        let mut map = ListOrderedMultimap::new();
        assert_eq!(map.entry_len(&"key"), 0);

        let already_exists = map.append("key", "value1");
        assert!(!already_exists);
        assert_eq!(map.entry_len(&"key"), 1);

        let already_exists = map.append("key", "value2");
        assert!(already_exists);
        assert_eq!(map.entry_len(&"key"), 2);

        let mut iter = map.get_all(&"key");
        assert_eq!(iter.next(), Some(&"value1"));
        assert_eq!(iter.next(), Some(&"value2"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_list_ordered_multimap_clear() {
        let mut map = ListOrderedMultimap::new();
        map.insert("key", "value");
        map.insert("key2", "value");

        map.clear();

        assert!(map.is_empty());
        assert_eq!(map.get(&"key"), None);
        assert_eq!(map.get(&"key2"), None);
    }

    #[test]
    fn test_list_ordered_multimap_contains_key() {
        let mut map = ListOrderedMultimap::new();
        assert!(!map.contains_key(&"key"));

        map.insert("key", "value");
        assert!(map.contains_key(&"key"));
    }

    #[test]
    fn test_list_ordered_multimap_entry() {
        let mut map = ListOrderedMultimap::new();
        assert_eq!(map.get(&"key1"), None);

        let value = map.entry("key").or_insert("value1");
        assert_eq!(value, &"value1");
        assert_eq!(map.get(&"key"), Some(&"value1"));

        let value = map.entry("key").or_insert("value2");
        assert_eq!(value, &"value1");
        assert_eq!(map.get(&"key"), Some(&"value1"));
    }

    #[test]
    fn test_list_ordered_multimap_entry_len() {
        let mut map = ListOrderedMultimap::new();
        assert_eq!(map.entry_len(&"key1"), 0);

        map.insert("key", "value");
        assert_eq!(map.entry_len(&"key"), 1);

        map.insert("key", "value");
        assert_eq!(map.entry_len(&"key"), 1);

        map.append("key", "value");
        assert_eq!(map.entry_len(&"key"), 2);

        map.insert("key", "value");
        assert_eq!(map.entry_len(&"key"), 1);

        map.remove(&"key");
        assert_eq!(map.entry_len(&"key"), 0);
    }

    #[test]
    fn test_list_ordered_multimap_get() {
        let mut map = ListOrderedMultimap::new();
        assert_eq!(map.get(&"key"), None);

        map.insert("key", "value");
        assert_eq!(map.get(&"key"), Some(&"value"));
    }

    #[test]
    fn test_list_ordered_multimap_get_all() {
        let mut map = ListOrderedMultimap::new();

        let mut iter = map.get_all(&"key");
        assert_eq!(iter.next(), None);

        map.insert("key", "value1");
        map.append("key", "value2");
        map.append("key", "value3");

        let mut iter = map.get_all(&"key");
        assert_eq!(iter.next(), Some(&"value1"));
        assert_eq!(iter.next(), Some(&"value2"));
        assert_eq!(iter.next(), Some(&"value3"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_list_ordered_multimap_get_all_mut() {
        let mut map = ListOrderedMultimap::new();

        let mut iter = map.get_all(&"key");
        assert_eq!(iter.next(), None);

        map.insert("key", "value1");
        map.append("key", "value2");
        map.append("key", "value3");

        let mut iter = map.get_all_mut(&"key");
        assert_eq!(iter.next(), Some(&mut "value1"));
        assert_eq!(iter.next(), Some(&mut "value2"));
        assert_eq!(iter.next(), Some(&mut "value3"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_list_ordered_multimap_get_mut() {
        let mut map = ListOrderedMultimap::new();
        assert_eq!(map.get_mut(&"key"), None);

        map.insert("key", "value");
        assert_eq!(map.get_mut(&"key"), Some(&mut "value"));
    }

    #[test]
    fn test_list_ordered_multimap_insert() {
        let mut map = ListOrderedMultimap::new();
        assert!(!map.contains_key(&"key"));
        assert_eq!(map.get(&"key"), None);

        let value = map.insert("key", "value1");
        assert_eq!(value, None);
        assert!(map.contains_key(&"key"));
        assert_eq!(map.get(&"key"), Some(&"value1"));

        let value = map.insert("key", "value2");
        assert_eq!(value, Some("value1"));
        assert!(map.contains_key(&"key"));
        assert_eq!(map.get(&"key"), Some(&"value2"));
    }

    #[test]
    fn test_list_ordered_multimap_insert_all() {
        let mut map = ListOrderedMultimap::new();
        assert!(!map.contains_key(&"key"));
        assert_eq!(map.get(&"key"), None);

        {
            let mut iter = map.insert_all("key", "value1");
            assert_eq!(iter.next(), None);
        }

        assert!(map.contains_key(&"key"));
        assert_eq!(map.get(&"key"), Some(&"value1"));

        {
            let mut iter = map.insert_all("key", "value2");
            assert_eq!(iter.next(), Some("value1"));
            assert_eq!(iter.next(), None);
        }

        assert!(map.contains_key(&"key"));
        assert_eq!(map.get(&"key"), Some(&"value2"));
    }

    #[test]
    fn test_list_ordered_multimap_is_empty() {
        let mut map = ListOrderedMultimap::new();
        assert!(map.is_empty());

        map.insert("key", "value");
        assert!(!map.is_empty());

        map.remove(&"key");
        assert!(map.is_empty());
    }

    #[test]
    fn test_list_ordered_multimap_iter() {
        let mut map = ListOrderedMultimap::new();

        let mut iter = map.iter();
        assert_eq!(iter.next(), None);

        map.insert("key1", "value1");
        map.insert("key2", "value2");
        map.append("key2", "value3");
        map.append("key1", "value4");

        let mut iter = map.iter();
        assert_eq!(iter.next(), Some((&"key1", &"value1")));
        assert_eq!(iter.next(), Some((&"key2", &"value2")));
        assert_eq!(iter.next(), Some((&"key2", &"value3")));
        assert_eq!(iter.next(), Some((&"key1", &"value4")));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_list_ordered_multimap_iter_mut() {
        let mut map = ListOrderedMultimap::new();

        let mut iter = map.iter_mut();
        assert_eq!(iter.next(), None);

        map.insert("key1", "value1");
        map.insert("key2", "value2");
        map.append("key2", "value3");
        map.append("key1", "value4");

        let mut iter = map.iter_mut();
        assert_eq!(iter.next(), Some((&"key1", &mut "value1")));
        assert_eq!(iter.next(), Some((&"key2", &mut "value2")));
        assert_eq!(iter.next(), Some((&"key2", &mut "value3")));
        assert_eq!(iter.next(), Some((&"key1", &mut "value4")));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_list_ordered_multimap_keys() {
        let mut map = ListOrderedMultimap::new();

        let mut iter = map.keys();
        assert_eq!(iter.next(), None);

        map.insert("key1", "value1");
        map.insert("key2", "value2");
        map.insert("key1", "value3");
        map.insert("key3", "value4");

        let mut iter = map.keys();
        assert_eq!(iter.next(), Some(&"key1"));
        assert_eq!(iter.next(), Some(&"key2"));
        assert_eq!(iter.next(), Some(&"key3"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_list_ordered_multimap_keys_capacity() {
        let mut map = ListOrderedMultimap::new();
        assert_eq!(map.keys_capacity(), 0);
        map.insert("key", "value");
        assert!(map.keys_capacity() > 0);
    }

    #[test]
    fn test_list_ordered_multimap_keys_len() {
        let mut map = ListOrderedMultimap::new();
        assert_eq!(map.keys_len(), 0);

        map.insert("key1", "value1");
        assert_eq!(map.keys_len(), 1);

        map.insert("key2", "value2");
        assert_eq!(map.keys_len(), 2);

        map.append("key1", "value3");
        assert_eq!(map.keys_len(), 2);

        map.remove(&"key1");
        assert_eq!(map.keys_len(), 1);

        map.remove(&"key2");
        assert_eq!(map.keys_len(), 0);
    }

    #[test]
    fn test_list_ordered_multimap_new() {
        let map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::new();
        assert_eq!(map.keys_capacity(), 0);
        assert_eq!(map.keys_len(), 0);
        assert_eq!(map.values_capacity(), 0);
        assert_eq!(map.values_len(), 0);
    }

    #[test]
    fn test_list_ordered_multimap_pack_to() {
        let mut map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::with_capacity(5, 5);
        map.pack_to_fit();
        assert_eq!(map.keys_capacity(), 0);
        assert_eq!(map.values_capacity(), 0);

        let mut map = ListOrderedMultimap::with_capacity(10, 10);

        map.insert("key1", "value1");
        map.insert("key2", "value2");
        map.append("key2", "value3");
        map.append("key1", "value4");

        map.pack_to(5, 5);
        assert_eq!(map.keys_capacity(), 5);
        assert_eq!(map.keys_len(), 2);
        assert_eq!(map.values_capacity(), 5);
        assert_eq!(map.values_len(), 4);

        let mut iter = map.iter();
        assert_eq!(iter.next(), Some((&"key1", &"value1")));
        assert_eq!(iter.next(), Some((&"key2", &"value2")));
        assert_eq!(iter.next(), Some((&"key2", &"value3")));
        assert_eq!(iter.next(), Some((&"key1", &"value4")));
        assert_eq!(iter.next(), None);
    }

    #[should_panic]
    #[test]
    fn test_list_ordered_multimap_pack_to_panic_key_capacity() {
        let mut map = ListOrderedMultimap::new();
        map.insert("key1", "value1");
        map.insert("key2", "value2");
        map.append("key2", "value3");
        map.append("key1", "value4");
        map.pack_to(1, 5);
    }

    #[should_panic]
    #[test]
    fn test_list_ordered_multimap_pack_to_panic_value_capacity() {
        let mut map = ListOrderedMultimap::new();
        map.insert("key1", "value1");
        map.insert("key2", "value2");
        map.append("key2", "value3");
        map.append("key1", "value4");
        map.pack_to(5, 1);
    }

    #[test]
    fn test_list_ordered_multimap_pack_to_fit() {
        let mut map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::with_capacity(5, 5);
        map.pack_to_fit();
        assert_eq!(map.keys_capacity(), 0);
        assert_eq!(map.values_capacity(), 0);

        let mut map = ListOrderedMultimap::with_capacity(5, 5);

        map.insert("key1", "value1");
        map.insert("key2", "value2");
        map.append("key2", "value3");
        map.append("key1", "value4");

        map.pack_to_fit();
        assert_eq!(map.keys_capacity(), 2);
        assert_eq!(map.keys_len(), 2);
        assert_eq!(map.values_capacity(), 4);
        assert_eq!(map.values_len(), 4);

        let mut iter = map.iter();
        assert_eq!(iter.next(), Some((&"key1", &"value1")));
        assert_eq!(iter.next(), Some((&"key2", &"value2")));
        assert_eq!(iter.next(), Some((&"key2", &"value3")));
        assert_eq!(iter.next(), Some((&"key1", &"value4")));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_list_ordered_multimap_remove() {
        let mut map = ListOrderedMultimap::new();
        assert_eq!(map.remove(&"key"), None);

        map.insert("key", "value1");
        map.append("key", "value2");
        assert_eq!(map.remove(&"key"), Some("value1"));
        assert_eq!(map.remove(&"key"), None);
    }

    #[test]
    fn test_list_ordered_multimap_remove_all() {
        let mut map = ListOrderedMultimap::new();

        {
            let mut iter = map.remove_all(&"key");
            assert_eq!(iter.next(), None);
        }

        map.insert("key", "value1");
        map.append("key", "value2");

        {
            let mut iter = map.remove_all(&"key");
            assert_eq!(iter.next(), Some("value1"));
            assert_eq!(iter.next(), Some("value2"));
            assert_eq!(iter.next(), None);
        }

        let mut iter = map.remove_all(&"key");
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_list_ordered_multimap_reserve_keys() {
        let mut map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::new();
        assert_eq!(map.keys_capacity(), 0);

        map.reserve_keys(5);
        assert!(map.keys_capacity() >= 5);

        let mut map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::with_capacity(5, 5);
        assert_eq!(map.keys_capacity(), 5);

        map.reserve_keys(2);
        assert_eq!(map.keys_capacity(), 5);
    }

    #[test]
    fn test_list_ordered_multimap_reserve_values() {
        let mut map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::new();
        assert_eq!(map.values_capacity(), 0);

        map.reserve_values(5);
        assert!(map.values_capacity() >= 5);

        let mut map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::with_capacity(5, 5);
        assert_eq!(map.values_capacity(), 5);

        map.reserve_values(2);
        assert_eq!(map.values_capacity(), 5);
    }

    #[test]
    fn test_list_ordered_multimap_values() {
        let mut map = ListOrderedMultimap::new();

        let mut iter = map.iter();
        assert_eq!(iter.next(), None);

        map.insert("key1", "value1");
        map.insert("key2", "value2");
        map.append("key2", "value3");
        map.append("key1", "value4");

        let mut iter = map.values();
        assert_eq!(iter.next(), Some(&"value1"));
        assert_eq!(iter.next(), Some(&"value2"));
        assert_eq!(iter.next(), Some(&"value3"));
        assert_eq!(iter.next(), Some(&"value4"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_list_ordered_multimap_values_mut() {
        let mut map = ListOrderedMultimap::new();

        let mut iter = map.iter();
        assert_eq!(iter.next(), None);

        map.insert("key1", "value1");
        map.insert("key2", "value2");
        map.append("key2", "value3");
        map.append("key1", "value4");

        let mut iter = map.values_mut();
        assert_eq!(iter.next(), Some(&mut "value1"));
        assert_eq!(iter.next(), Some(&mut "value2"));
        assert_eq!(iter.next(), Some(&mut "value3"));
        assert_eq!(iter.next(), Some(&mut "value4"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_list_ordered_multimap_values_capacity() {
        let mut map = ListOrderedMultimap::new();
        assert_eq!(map.values_capacity(), 0);
        map.insert("key", "value");
        assert!(map.values_capacity() > 0);
    }

    #[test]
    fn test_list_ordered_multimap_values_len() {
        let mut map = ListOrderedMultimap::new();
        assert_eq!(map.values_len(), 0);

        map.insert("key1", "value1");
        assert_eq!(map.values_len(), 1);

        map.insert("key2", "value2");
        assert_eq!(map.values_len(), 2);

        map.append("key1", "value3");
        assert_eq!(map.values_len(), 3);

        map.remove(&"key1");
        assert_eq!(map.values_len(), 1);

        map.remove(&"key2");
        assert_eq!(map.values_len(), 0);
    }

    #[test]
    fn test_list_ordered_multimap_with_capacity() {
        let map: ListOrderedMultimap<&str, &str> = ListOrderedMultimap::with_capacity(1, 2);
        assert!(map.keys_capacity() >= 1);
        assert_eq!(map.keys_len(), 0);
        assert!(map.values_capacity() >= 2);
        assert_eq!(map.values_len(), 0);
    }

    #[test]
    fn test_list_ordered_multimap_with_capacity_and_hasher() {
        let state = RandomState::new();
        let map: ListOrderedMultimap<&str, &str> =
            ListOrderedMultimap::with_capacity_and_hasher(1, 2, state);
        assert!(map.keys_capacity() >= 1);
        assert_eq!(map.keys_len(), 0);
        assert!(map.values_capacity() >= 2);
        assert_eq!(map.values_len(), 0);
    }
}
