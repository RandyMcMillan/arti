//! A data structure for the solver's bucket sort layers
//!
//! You could imagine using an array of [`arrayvec::ArrayVec`] for this
//! same purpose, but the resulting memory layout from that approach
//! would be quite bad; we want to keep parallel arrays for bucket counts,
//! keys, and values.
//!
//! This 'bucket array' additionally supports several memory optimizations we
//! like to have: splitting the storage into a reusable part and a disposable
//! part, adjustable data types for everything, and support for dropping our
//! key storage memory separately from the rest.
//!
//! The reusable memory regions are always considered uninitialized when not
//! wrapped by a bucket array instance, which then performs its own tracking.

use num_traits::{One, WrappingAdd, WrappingNeg, Zero};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::{Add, BitAnd, Div, Mul, Not, Range, Rem, Shl, Shr, Sub};

/// Backing memory for a single key or value bucket array.
///
/// Describes N buckets which each hold at most M items of type T.
/// There's no constructor, it must be created using unsafe code that
/// calls [`std::alloc::alloc()`] or similar. It is always assumed to be
/// uninitialized unless a mutable reference is held and it's been
/// initialized by the holder of that reference.
#[derive(Copy, Clone)]
pub(crate) struct BucketArrayMemory<const N: usize, const M: usize, T: Copy> {
    /// Arrays of [`MaybeUninit`], always considered uninitialized unless we
    /// are using a specific mutable reference to manipulate this memory.
    inner: [[MaybeUninit<T>; M]; N],
}

/// Trait for accessing the overall shape of a bucket array
pub(crate) trait Shape<K: Key> {
    /// The number of buckets in this array
    const NUM_BUCKETS: usize;

    /// The item capacity of each bucket
    const BUCKET_CAPACITY: usize;

    /// Get the range of items within a single bucket
    fn item_range(&self, bucket: usize) -> Range<usize>;

    /// Get the key divisor, the number of buckets but as a [`Key`] instance
    #[inline(always)]
    fn divisor(&self) -> K {
        K::from_bucket_index(Self::NUM_BUCKETS)
    }

    /// Split a wide key into the bucket index and the remaining bits
    #[inline(always)]
    fn split_wide_key(&self, key: K) -> (usize, K) {
        let divisor = self.divisor();
        ((key % divisor).into_bucket_index(), (key / divisor))
    }

    /// Rebuild a wide key from its split components
    #[inline(always)]
    fn join_wide_key(&self, bucket: usize, remainder: K) -> K {
        let divisor = self.divisor();
        remainder * divisor + K::from_bucket_index(bucket)
    }
}

/// Trait for writing new key/value pairs to a bucket array.
pub(crate) trait Insert<K: Key, V: Copy> {
    /// Append a new item to its sorting bucket, or return Err(()) if it's full
    fn insert(&mut self, key: K, value: V) -> Result<(), ()>;
}

/// Trait for bucket arrays that include storage for keys.
/// Keys are always used to index into the bucket array, but an array may
/// also optionally include storage for the remaining portion.
pub(crate) trait KeyLookup<S: KeyStorage<K>, K: Key> {
    /// Retrieve the stored key remainder bits for this item
    fn item_stored_key(&self, bucket: usize, item: usize) -> S;

    /// Retrieve the key for a particular item, as a full width key
    fn item_full_key(&self, bucket: usize, item: usize) -> K;
}

/// Trait for bucket arrays that include storage for values.
/// Values are opaque data, any [`Copy`] type may be used.
pub(crate) trait ValueLookup<V: Copy> {
    /// Retrieve the Value for a particular item
    fn item_value(&self, bucket: usize, item: usize) -> V;
}

/// Common implementation for key/value and value-only bucket arrays.
/// Tracks the number of items in each bucket.
struct BucketArrayImpl<const N: usize, const CAP: usize, C: Count, K: Key> {
    /// Number of initialized items in each bucket.
    /// Each bucket B's valid item range would be `(0 .. counts[B])`
    counts: [C; N],
    /// Each bucket array implementation works with a specific key type but
    /// never stores them directly. Keys may or may not be stored, and when
    /// they are it's managed by a [`KeyStorage`] type.
    phantom: PhantomData<K>,
}

impl<const N: usize, const CAP: usize, C: Count, K: Key> BucketArrayImpl<N, CAP, C, K> {
    /// Capacity of each bucket in the array
    const BUCKET_CAPACITY: usize = CAP;

    /// Create a new counter store. This will happen inside the lifetime
    /// of our mutable reference to the backing store memory.
    fn new() -> Self {
        Self {
            counts: [C::zero(); N],
            phantom: PhantomData,
        }
    }

    /// Look up the valid item range for a particular bucket.
    /// Panics if the bucket index is out of range. Item indices inside the
    /// returned range are initialized, and any outside may be uninitialized.
    #[inline(always)]
    fn item_range(&self, bucket: usize) -> Range<usize> {
        0..self.counts[bucket].into()
    }

    /// Append a new item to a specific bucket, using a writer callback.
    /// The writer is invoked with an item index, after checking
    /// bucket capacity but before marking the new item as written.
    #[inline(always)]
    fn insert<F: FnMut(usize)>(&mut self, bucket: usize, mut writer: F) -> Result<(), ()> {
        let item_count = self.counts[bucket];
        let count_usize: usize = item_count.into();
        if count_usize < Self::BUCKET_CAPACITY {
            writer(count_usize);
            self.counts[bucket] = item_count + C::one();
            Ok(())
        } else {
            Err(())
        }
    }
}

/// Concrete bucket array with parallel [`BucketArrayMemory`] for key and value
/// storage. This is the basic data type used for one layer of sorting buckets.
///
/// Takes several type parameters so the narrowest possible types can be
/// chosen for counters, keys, and values. Keys take two types, the 'wide'
/// version that appears in the API and a 'storage' version that's been
/// stripped of the data that's redundant with bucket position.
///
/// The validity of [`BucketArrayMemory`] entries is ensured by the combiation
/// of our mutable ref to the `BucketArrayMemory` itself and our tracking of
/// bucket counts within the lifetime of that reference.
pub(crate) struct KeyValueBucketArray<
    'k,
    'v,
    const N: usize,
    const CAP: usize,
    C: Count,
    K: Key,
    KS: KeyStorage<K>,
    V: Copy,
> {
    /// Reference to external backing memory for KeyStorage
    key_mem: &'k mut BucketArrayMemory<N, CAP, KS>,
    /// Reference to external backing memory for values
    value_mem: &'v mut BucketArrayMemory<N, CAP, V>,
    /// Inner implementation and bucket counters
    inner: BucketArrayImpl<N, CAP, C, K>,
}

impl<'k, 'v, const N: usize, const CAP: usize, C: Count, K: Key, KS: KeyStorage<K>, V: Copy>
    KeyValueBucketArray<'k, 'v, N, CAP, C, K, KS, V>
{
    /// A new [`KeyValueBucketArray`] wraps two mutable [`BucketArrayMemory`]
    /// references and adds a counts array to track which items are valid.
    pub(crate) fn new(
        key_mem: &'k mut BucketArrayMemory<N, CAP, KS>,
        value_mem: &'v mut BucketArrayMemory<N, CAP, V>,
    ) -> Self {
        Self {
            key_mem,
            value_mem,
            inner: BucketArrayImpl::new(),
        }
    }

    /// Keep the counts and the value memory but drop the key memory. Returns
    /// a new [`ValueBucketArray`].
    pub(crate) fn drop_key_storage(self) -> ValueBucketArray<'v, N, CAP, C, K, V> {
        ValueBucketArray {
            value_mem: self.value_mem,
            inner: self.inner,
        }
    }
}

/// Concrete bucket array with a single [`BucketArrayMemory`] for value storage.
/// Keys are used for bucket indexing but the remainder bits are not stored.
pub(crate) struct ValueBucketArray<'v, const N: usize, const CAP: usize, C: Count, K: Key, V: Copy>
{
    /// Reference to external backing memory for values
    value_mem: &'v mut BucketArrayMemory<N, CAP, V>,
    /// Inner implementation and bucket counters
    inner: BucketArrayImpl<N, CAP, C, K>,
}

impl<'v, const N: usize, const CAP: usize, C: Count, K: Key, V: Copy>
    ValueBucketArray<'v, N, CAP, C, K, V>
{
    /// A new [`ValueBucketArray`] wraps one mutable [`BucketArrayMemory`]
    /// reference and adds a counts array to track which items are valid.
    pub(crate) fn new(value_mem: &'v mut BucketArrayMemory<N, CAP, V>) -> Self {
        Self {
            value_mem,
            inner: BucketArrayImpl::new(),
        }
    }
}

impl<'k, 'v, const N: usize, const CAP: usize, C: Count, K: Key, KS: KeyStorage<K>, V: Copy>
    Shape<K> for KeyValueBucketArray<'k, 'v, N, CAP, C, K, KS, V>
{
    /// Number of buckets in the array
    const NUM_BUCKETS: usize = N;
    /// Capacity of each bucket in the array
    const BUCKET_CAPACITY: usize = CAP;

    #[inline(always)]
    fn item_range(&self, bucket: usize) -> Range<usize> {
        self.inner.item_range(bucket)
    }
}

impl<'v, const N: usize, const CAP: usize, C: Count, K: Key, V: Copy> Shape<K>
    for ValueBucketArray<'v, N, CAP, C, K, V>
{
    /// Number of buckets in the array
    const NUM_BUCKETS: usize = N;
    /// Capacity of each bucket in the array
    const BUCKET_CAPACITY: usize = CAP;

    #[inline(always)]
    fn item_range(&self, bucket: usize) -> Range<usize> {
        self.inner.item_range(bucket)
    }
}

impl<'k, 'v, const N: usize, const CAP: usize, C: Count, K: Key, KS: KeyStorage<K>, V: Copy>
    Insert<K, V> for KeyValueBucketArray<'k, 'v, N, CAP, C, K, KS, V>
{
    #[inline(always)]
    fn insert(&mut self, key: K, value: V) -> Result<(), ()> {
        let (bucket, key_remainder) = self.split_wide_key(key);
        self.inner.insert(bucket, |item| {
            let key_storage = KS::from_key(key_remainder);
            self.key_mem.inner[bucket][item].write(key_storage);
            self.value_mem.inner[bucket][item].write(value);
        })
    }
}

impl<'v, const N: usize, const CAP: usize, C: Count, K: Key, V: Copy> Insert<K, V>
    for ValueBucketArray<'v, N, CAP, C, K, V>
{
    #[inline(always)]
    fn insert(&mut self, key: K, value: V) -> Result<(), ()> {
        let (bucket, _) = self.split_wide_key(key);
        self.inner.insert(bucket, |item| {
            self.value_mem.inner[bucket][item].write(value);
        })
    }
}

impl<'k, 'v, const N: usize, const CAP: usize, C: Count, K: Key, KS: KeyStorage<K>, V: Copy>
    KeyLookup<KS, K> for KeyValueBucketArray<'k, 'v, N, CAP, C, K, KS, V>
{
    #[inline(always)]
    fn item_stored_key(&self, bucket: usize, item: usize) -> KS {
        assert!(self.inner.item_range(bucket).contains(&item));
        unsafe { self.key_mem.inner[bucket][item].assume_init() }
    }

    #[inline(always)]
    fn item_full_key(&self, bucket: usize, item: usize) -> K {
        self.join_wide_key(bucket, self.item_stored_key(bucket, item).into_key())
    }
}

impl<'k, 'v, const N: usize, const CAP: usize, C: Count, K: Key, KS: KeyStorage<K>, V: Copy>
    ValueLookup<V> for KeyValueBucketArray<'k, 'v, N, CAP, C, K, KS, V>
{
    #[inline(always)]
    fn item_value(&self, bucket: usize, item: usize) -> V {
        assert!(self.inner.item_range(bucket).contains(&item));
        unsafe { self.value_mem.inner[bucket][item].assume_init() }
    }
}

impl<'v, const N: usize, const CAP: usize, C: Count, K: Key, V: Copy> ValueLookup<V>
    for ValueBucketArray<'v, N, CAP, C, K, V>
{
    #[inline(always)]
    fn item_value(&self, bucket: usize, item: usize) -> V {
        assert!(self.inner.item_range(bucket).contains(&item));
        unsafe { self.value_mem.inner[bucket][item].assume_init() }
    }
}

/// Types that can be used as a count of items in a bucket
pub(crate) trait Count:
    Copy + Zero + One + TryFrom<usize> + Into<usize> + Add<Self, Output = Self>
{
    /// Convert from a usize item index, panic on overflow
    #[inline(always)]
    fn from_item_index(i: usize) -> Self {
        // Replace the original error type, to avoid propagating Debug bounds
        // for this trait. We might be able to stop doing this once the
        // associated_type_bounds Rust feature stabilizes.
        i.try_into()
            .map_err(|_| ())
            .expect("Bucket count type is always wide enough for item index")
    }
}

impl<T: Copy + Zero + One + TryFrom<usize> + Into<usize> + Add<Self, Output = Self>> Count for T {}

/// Types we can use as full width keys
pub(crate) trait Key:
    Copy
    + Zero
    + One
    + PartialEq<Self>
    + TryFrom<usize>
    + TryInto<usize>
    + Shl<usize, Output = Self>
    + Shr<usize, Output = Self>
    + Div<Self, Output = Self>
    + Rem<Self, Output = Self>
    + Mul<Self, Output = Self>
    + Not
    + Sub<Self, Output = Self>
    + BitAnd<Self, Output = Self>
    + WrappingAdd
    + WrappingNeg
{
    /// Build a Key from a bucket index, panics if it is out of range
    #[inline(always)]
    fn from_bucket_index(i: usize) -> Self {
        i.try_into()
            .map_err(|_| ())
            .expect("Key type is always wide enough for a bucket index")
    }

    /// Convert this Key back into a bucket index, panics if it is out of range
    #[inline(always)]
    fn into_bucket_index(self) -> usize {
        self.try_into()
            .map_err(|_| ())
            .expect("Key is a bucket index which always fits in a usize")
    }

    /// Check if the N low bits of the key are zero
    #[inline(always)]
    fn low_bits_are_zero(self, num_bits: usize) -> bool {
        (self & ((Self::one() << num_bits) - Self::one())) == Self::zero()
    }
}

impl<
        T: Copy
            + Zero
            + One
            + PartialEq<Self>
            + TryFrom<usize>
            + TryInto<usize>
            + Shl<usize, Output = Self>
            + Shr<usize, Output = Self>
            + Div<Self, Output = Self>
            + Rem<Self, Output = Self>
            + Mul<Self, Output = Self>
            + Not
            + Sub<Self, Output = Self>
            + BitAnd<Self, Output = Self>
            + WrappingAdd
            + WrappingNeg,
    > Key for T
{
}

/// Backing storage for a specific key type. Intended to be smaller
/// than or equal in size to the full Key type.
pub(crate) trait KeyStorage<K>:
    Copy + Zero + Not<Output = Self> + TryFrom<K> + TryInto<K>
where
    K: Key,
{
    /// Fit the indicated key into a [`KeyStorage`], wrapping if necessary.
    /// It is normal for keys to accumulate additional insignificant bits on
    /// the left side as we compute sums.
    #[inline(always)]
    fn from_key(k: K) -> Self {
        let key_mask = (!Self::zero()).into_key();
        <K as TryInto<Self>>::try_into(k & key_mask)
            .map_err(|_| ())
            .expect("masked Key type always fits in KeyStorage")
    }

    /// Unpack this [`KeyStorage`] back into a Key type, without
    /// changing its value.
    #[inline(always)]
    fn into_key(self) -> K {
        self.try_into()
            .map_err(|_| ())
            .expect("Key type is always wider than KeyStorage")
    }
}

impl<T: Copy + Zero + Not<Output = Self> + TryFrom<K> + TryInto<K>, K: Key> KeyStorage<K> for T {}
