//! All the traits of this crate.

use downcast_rs::{impl_downcast, Downcast};
use rand::RngCore;
use ssh_key::{
    private::{Ed25519Keypair, Ed25519PrivateKey, KeypairData, OpaqueKeypair},
    public::{Ed25519PublicKey, KeyData, OpaquePublicKey},
    rand_core::CryptoRng,
    Algorithm, AlgorithmName,
};
use tor_error::internal;
use tor_hscrypto::pk::{
    HsBlindIdKey, HsBlindIdKeypair, HsClientDescEncKeypair, HsDescSigningKeypair, HsIdKey,
    HsIdKeypair, HsIntroPtSessionIdKeypair, HsSvcNtorKeypair,
};
use tor_llcrypto::pk::{curve25519, ed25519};

use crate::{
    ssh::{SshKeyData, ED25519_EXPANDED_ALGORITHM_NAME, X25519_ALGORITHM_NAME},
    KeyType, Result,
};

/// A random number generator for generating [`EncodableKey`]s.
pub trait KeygenRng: RngCore + CryptoRng {}

impl<T> KeygenRng for T where T: RngCore + CryptoRng {}

/// A trait for generating fresh keys.
pub trait Keygen {
    /// Generate a new key of this type.
    fn generate(rng: &mut dyn KeygenRng) -> Result<Self>
    where
        Self: Sized;
}

/// A key that can be serialized to, and deserialized from.
//
// When adding a new `EncodableKey` impl, you must also update
// [`SshKeyData::into_erased`](crate::SshKeyData::into_erased) to
// return the corresponding concrete type implementing `EncodableKey`
// (as a `dyn EncodableKey`).
pub trait EncodableKey: Downcast {
    /// The type of the key.
    fn key_type() -> KeyType
    where
        Self: Sized;

    /// Return the [`SshKeyData`] of this key.
    fn as_ssh_key_data(&self) -> Result<SshKeyData>;
}

impl_downcast!(EncodableKey);

/// A key that can be converted to an [`EncodableKey`].
//
// NOTE: Conceptually, the `ToEncodableKey` and `EncodableKey` traits serve the same purpose (they
// provide information about how to encode/decode a key).
//
// The reason we have two traits instead of just one is because `EncodableKey` cannot have an
// associated type: for instance, if it did, we'd need to either give
// `tor-keymgr::Keystore::insert` a generic parameter (which would make `Keystore` object-unsafe),
// or specify a concrete type for the associated type of the `EncodableKey` (which would defeat the
// whole purpose of the trait, i.e. to enable users to store their own "encodable key" types).
//
// `ToEncodableKey` is used in the `KeyMgr` impl, where the associated type isn't an issue because
// the `KeyMgr` implementation is generic over `K: ToEncodableKey`. The `Keystore`s themselves only
// receive `&dyn EncodableKey`s.

pub trait ToEncodableKey: From<Self::KeyPair>
where
    Self::Key: From<<Self::KeyPair as ToEncodableKey>::Key>,
{
    /// The key type this can be converted to/from.
    type Key: EncodableKey + 'static;

    /// The KeyPair (secret+public) of which this key is a subset.  For secret
    /// keys, this type is Self.  For public keys, this type is the
    /// corresponding (secret) keypair.
    ///
    /// The associated type constraint (`where`) expresses the fact that a
    /// public key is always derivable from its corresponding secret key.
    ///
    type KeyPair: ToEncodableKey;

    /// Convert this key to a type that implements [`EncodableKey`].
    fn to_encodable_key(self) -> Self::Key;

    /// Convert an [`EncodableKey`] to another key type.
    fn from_encodable_key(key: Self::Key) -> Self;
}

impl Keygen for curve25519::StaticKeypair {
    fn generate(rng: &mut dyn KeygenRng) -> Result<Self>
    where
        Self: Sized,
    {
        let secret = curve25519::StaticSecret::random_from_rng(rng);
        let public = curve25519::PublicKey::from(&secret);

        Ok(curve25519::StaticKeypair { secret, public })
    }
}

impl EncodableKey for curve25519::StaticKeypair {
    fn key_type() -> KeyType
    where
        Self: Sized,
    {
        KeyType::X25519StaticKeypair
    }

    fn as_ssh_key_data(&self) -> Result<SshKeyData> {
        let algorithm_name = AlgorithmName::new(X25519_ALGORITHM_NAME)
            .map_err(|_| internal!("invalid algorithm name"))?;

        let ssh_public = OpaquePublicKey::new(
            self.public.to_bytes().to_vec(),
            Algorithm::Other(algorithm_name),
        );
        let keypair = OpaqueKeypair::new(self.secret.to_bytes().to_vec(), ssh_public);

        SshKeyData::try_from_keypair_data(KeypairData::Other(keypair))
    }
}

impl EncodableKey for curve25519::PublicKey {
    fn key_type() -> KeyType
    where
        Self: Sized,
    {
        KeyType::X25519PublicKey
    }

    fn as_ssh_key_data(&self) -> Result<SshKeyData> {
        let algorithm_name = AlgorithmName::new(X25519_ALGORITHM_NAME)
            .map_err(|_| internal!("invalid algorithm name"))?;

        let ssh_public =
            OpaquePublicKey::new(self.to_bytes().to_vec(), Algorithm::Other(algorithm_name));

        SshKeyData::try_from_key_data(KeyData::Other(ssh_public))
    }
}

impl Keygen for ed25519::Keypair {
    fn generate(mut rng: &mut dyn KeygenRng) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(ed25519::Keypair::generate(&mut rng))
    }
}

impl EncodableKey for ed25519::Keypair {
    fn key_type() -> KeyType
    where
        Self: Sized,
    {
        KeyType::Ed25519Keypair
    }

    fn as_ssh_key_data(&self) -> Result<SshKeyData> {
        let keypair = Ed25519Keypair {
            public: Ed25519PublicKey(self.verifying_key().to_bytes()),
            private: Ed25519PrivateKey::from_bytes(self.as_bytes()),
        };

        SshKeyData::try_from_keypair_data(KeypairData::Ed25519(keypair))
    }
}

impl EncodableKey for ed25519::PublicKey {
    fn key_type() -> KeyType
    where
        Self: Sized,
    {
        KeyType::Ed25519PublicKey
    }

    fn as_ssh_key_data(&self) -> Result<SshKeyData> {
        let key_data = Ed25519PublicKey(self.to_bytes());

        SshKeyData::try_from_key_data(ssh_key::public::KeyData::Ed25519(key_data))
    }
}

impl Keygen for ed25519::ExpandedKeypair {
    fn generate(rng: &mut dyn KeygenRng) -> Result<Self>
    where
        Self: Sized,
    {
        let keypair = <ed25519::Keypair as Keygen>::generate(rng)?;

        Ok((&keypair).into())
    }
}

impl EncodableKey for ed25519::ExpandedKeypair {
    fn key_type() -> KeyType
    where
        Self: Sized,
    {
        KeyType::Ed25519ExpandedKeypair
    }

    fn as_ssh_key_data(&self) -> Result<SshKeyData> {
        let algorithm_name = AlgorithmName::new(ED25519_EXPANDED_ALGORITHM_NAME)
            .map_err(|_| internal!("invalid algorithm name"))?;

        let ssh_public = OpaquePublicKey::new(
            self.public().to_bytes().to_vec(),
            Algorithm::Other(algorithm_name),
        );

        let keypair = OpaqueKeypair::new(self.to_secret_key_bytes().to_vec(), ssh_public);

        SshKeyData::try_from_keypair_data(KeypairData::Other(keypair))
    }
}

// TODO: These need to be put into the tor-hscrypto crate and using the deftly macro for key
// wrappers. We lack curve25519 support for such macro and so for now we move this code from
// tor-keymgr as it is.

impl ToEncodableKey for HsClientDescEncKeypair {
    type Key = curve25519::StaticKeypair;
    type KeyPair = HsClientDescEncKeypair;

    fn to_encodable_key(self) -> Self::Key {
        self.into()
    }

    fn from_encodable_key(key: Self::Key) -> Self {
        HsClientDescEncKeypair::new(key.public.into(), key.secret.into())
    }
}

impl ToEncodableKey for HsBlindIdKeypair {
    type Key = ed25519::ExpandedKeypair;
    type KeyPair = HsBlindIdKeypair;

    fn to_encodable_key(self) -> Self::Key {
        self.into()
    }

    fn from_encodable_key(key: Self::Key) -> Self {
        HsBlindIdKeypair::from(key)
    }
}

impl ToEncodableKey for HsBlindIdKey {
    type Key = ed25519::PublicKey;
    type KeyPair = HsBlindIdKeypair;

    fn to_encodable_key(self) -> Self::Key {
        self.into()
    }

    fn from_encodable_key(key: Self::Key) -> Self {
        HsBlindIdKey::from(key)
    }
}

impl ToEncodableKey for HsIdKeypair {
    type Key = ed25519::ExpandedKeypair;
    type KeyPair = HsIdKeypair;

    fn to_encodable_key(self) -> Self::Key {
        self.into()
    }

    fn from_encodable_key(key: Self::Key) -> Self {
        HsIdKeypair::from(key)
    }
}

impl ToEncodableKey for HsIdKey {
    type Key = ed25519::PublicKey;
    type KeyPair = HsIdKeypair;

    fn to_encodable_key(self) -> Self::Key {
        self.into()
    }

    fn from_encodable_key(key: Self::Key) -> Self {
        HsIdKey::from(key)
    }
}

impl ToEncodableKey for HsDescSigningKeypair {
    type Key = ed25519::Keypair;
    type KeyPair = HsDescSigningKeypair;

    fn to_encodable_key(self) -> Self::Key {
        self.into()
    }

    fn from_encodable_key(key: Self::Key) -> Self {
        HsDescSigningKeypair::from(key)
    }
}

impl ToEncodableKey for HsIntroPtSessionIdKeypair {
    type Key = ed25519::Keypair;
    type KeyPair = HsIntroPtSessionIdKeypair;

    fn to_encodable_key(self) -> Self::Key {
        self.into()
    }

    fn from_encodable_key(key: Self::Key) -> Self {
        key.into()
    }
}

impl ToEncodableKey for HsSvcNtorKeypair {
    type Key = curve25519::StaticKeypair;
    type KeyPair = HsSvcNtorKeypair;

    fn to_encodable_key(self) -> Self::Key {
        self.into()
    }

    fn from_encodable_key(key: Self::Key) -> Self {
        key.into()
    }
}
