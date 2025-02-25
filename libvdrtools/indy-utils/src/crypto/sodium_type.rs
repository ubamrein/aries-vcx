// This macro allows to wrap Sodimoxide type to libvdrtools type keeping the same behaviour
#[macro_export]
macro_rules! sodium_type (($newtype:ident, $sodiumtype:path, $len:ident) => (
    pub struct $newtype(pub(super) $sodiumtype);

    impl $newtype {

        #[allow(dead_code)]
        pub fn new(bytes: [u8; $len]) -> $newtype {
            $newtype($sodiumtype(bytes))
        }

        #[allow(dead_code)]
        pub fn from_slice(bs: &[u8]) -> Result<$newtype, indy_api_types::errors::IndyError> {
            let inner = <$sodiumtype>::from_slice(bs)
                .ok_or(indy_api_types::errors::err_msg(indy_api_types::errors::IndyErrorKind::InvalidStructure, format!("Invalid bytes for {:?}", stringify!($newtype))))?;

            Ok($newtype(inner))
        }
    }

    impl Clone for $newtype {
        fn clone(&self) -> $newtype {
            $newtype(self.0.clone())
        }
    }

    impl ::std::fmt::Debug for $newtype {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }

    impl ::std::cmp::PartialEq for $newtype {
        fn eq(&self, other: &$newtype) -> bool {
            self.0.eq(&other.0)
        }
    }

    impl ::std::cmp::Eq for $newtype {}

    impl ::serde::Serialize for $newtype {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: ::serde::Serializer
        {
            serializer.serialize_bytes(&self.0[..])
        }
    }

    impl<'de> ::serde::Deserialize<'de> for $newtype {
        fn deserialize<D>(deserializer: D) -> Result<$newtype, D::Error> where D: ::serde::Deserializer<'de>
        {
            <$sodiumtype>::deserialize(deserializer).map($newtype)
        }
    }

    impl ::std::ops::Index<::std::ops::Range<usize>> for $newtype {
        type Output = [u8];

        fn index(&self, _index: ::std::ops::Range<usize>) -> &[u8] {
            self.0.index(_index)
        }
    }

    impl ::std::ops::Index<::std::ops::RangeTo<usize>> for $newtype {
        type Output = [u8];

        fn index(&self, _index: ::std::ops::RangeTo<usize>) -> &[u8] {
            self.0.index(_index)
        }
    }

    impl ::std::ops::Index<::std::ops::RangeFrom<usize>> for $newtype {
        type Output = [u8];

        fn index(&self, _index: ::std::ops::RangeFrom<usize>) -> &[u8] {
            self.0.index(_index)
        }
    }

    impl ::std::ops::Index<::std::ops::RangeFull> for $newtype {
        type Output = [u8];

        fn index(&self, _index: ::std::ops::RangeFull) -> &[u8] {
            self.0.index(_index)
        }
    }

    impl AsRef<[u8]> for $newtype {
        #[inline]
        fn as_ref(&self) -> &[u8] {
            &self[..]
        }
    }
));


#[macro_export]
macro_rules! sodium_type_from_bytes (($newtype:ident, $sodiumtype:path, $len:ident) => (
    pub struct $newtype(pub(super) $sodiumtype);

    impl $newtype {

        #[allow(dead_code)]
        pub fn new(bytes: [u8; $len]) -> $newtype {
            $newtype(<$sodiumtype>::from_bytes(&bytes).unwrap())
        }

        #[allow(dead_code)]
        pub fn from_slice(bs: &[u8]) -> Result<$newtype, indy_api_types::errors::IndyError> {
            let inner = <$sodiumtype>::from_bytes(bs)
                .map_err(|_|indy_api_types::errors::err_msg(indy_api_types::errors::IndyErrorKind::InvalidStructure, format!("Invalid bytes for {:?}", stringify!($newtype))))?;

            Ok($newtype(inner))
        }
    }

    impl Clone for $newtype {
        fn clone(&self) -> $newtype {
            $newtype(self.0.clone())
        }
    }

    impl ::std::fmt::Debug for $newtype {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }

    impl ::std::cmp::PartialEq for $newtype {
        fn eq(&self, other: &$newtype) -> bool {
            self.0.eq(&other.0)
        }
    }

    impl ::std::cmp::Eq for $newtype {}

    impl ::serde::Serialize for $newtype {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: ::serde::Serializer
        {
            serializer.serialize_bytes(&self.0.to_bytes()[..])
        }
    }

    // impl<'de> ::serde::Deserialize<'de> for $newtype {
    //     fn deserialize<D>(deserializer: D) -> Result<$newtype, D::Error> where D: ::serde::Deserializer<'de>
    //     {
    //         <[u8;<$sodiumtype>::BYTE_SIZE]>::deserialize(deserializer).map($newtype)
    //     }
    // }

    impl ::std::ops::Index<::std::ops::Range<usize>> for $newtype {
        type Output = [u8];

        fn index(&self, _index: ::std::ops::Range<usize>) -> &[u8] {
            self.0.as_ref().index(_index)
        }
    }

    impl ::std::ops::Index<::std::ops::RangeTo<usize>> for $newtype {
        type Output = [u8];

        fn index(&self, _index: ::std::ops::RangeTo<usize>) -> &[u8] {
            self.0.as_ref().index(_index)
        }
    }

    impl ::std::ops::Index<::std::ops::RangeFrom<usize>> for $newtype {
        type Output = [u8];

        fn index(&self, _index: ::std::ops::RangeFrom<usize>) -> &[u8] {
            self.0.as_ref().index(_index)
        }
    }

    impl ::std::ops::Index<::std::ops::RangeFull> for $newtype {
        type Output = [u8];

        fn index(&self, _index: ::std::ops::RangeFull) -> &[u8] {
            self.0.as_ref().index(_index)
        }
    }

    impl AsRef<[u8]> for $newtype {
        #[inline]
        fn as_ref(&self) -> &[u8] {
            &self[..]
        }
    }
));
