use serde::ser::{SerializeSeq, SerializeTuple};
use serde::Serialize;
pub use serde_state_derive::{DeserializeState, SerializeState};
use std::boxed::Box;
use std::marker::PhantomData;

pub trait SerializeState<State: ?Sized> {
    fn serialize_state<S>(&self, state: &State, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer;
}

pub trait DeserializeState<'de, State: ?Sized>: Sized {
    fn deserialize_state<D>(state: &State, deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>;
}

impl<State: ?Sized, T: SerializeState<State> + ?Sized> SerializeState<State> for &'_ T {
    fn serialize_state<S>(&self, state: &State, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        T::serialize_state(&**self, state, serializer)
    }
}

impl<State: ?Sized, T: SerializeState<State> + ?Sized> SerializeState<State> for Box<T> {
    fn serialize_state<S>(&self, state: &State, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        T::serialize_state(&**self, state, serializer)
    }
}
impl<'de, State: ?Sized, T> DeserializeState<'de, State> for Box<T>
where
    T: DeserializeState<'de, State>,
{
    fn deserialize_state<D>(state: &State, deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        T::deserialize_state(state, deserializer).map(Box::new)
    }
}

impl<State: ?Sized, T> SerializeState<State> for PhantomData<T> {
    fn serialize_state<S>(&self, _state: &State, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_unit_struct("PhantomData")
    }
}
impl<'de, State: ?Sized, T> DeserializeState<'de, State> for PhantomData<T> {
    fn deserialize_state<D>(_state: &State, deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::Deserialize;
        let _ = <serde::de::IgnoredAny>::deserialize(deserializer)?;
        Ok(PhantomData)
    }
}

macro_rules! impl_state_passthrough {
    ($($ty:ty),* $(,)?) => {
        $(
            impl<State: ?Sized> SerializeState<State> for $ty {
                fn serialize_state<S>(
                    &self,
                    _state: &State,
                    serializer: S,
                ) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer,
                {
                    serde::Serialize::serialize(self, serializer)
                }
            }

            impl<'de, State: ?Sized> DeserializeState<'de, State> for $ty {
                fn deserialize_state<D>(
                    _state: &State,
                    deserializer: D,
                ) -> Result<Self, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    serde::Deserialize::deserialize(deserializer)
                }
            }
        )*
    };
}

impl_state_passthrough!(
    bool, char, String, u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize,
);

/// A value with attached state. Its Serialize impl calls `T`'s SerializeState impl.
pub struct WithState<'state, T, State: ?Sized> {
    value: T,
    state: &'state State,
}

impl<'state, T, State> WithState<'state, T, State>
where
    State: ?Sized,
{
    pub fn new(value: T, state: &'state State) -> Self {
        Self { value, state }
    }
}

impl<T, State: ?Sized> Serialize for WithState<'_, T, State>
where
    T: SerializeState<State>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.value.serialize_state(self.state, serializer)
    }
}

pub mod __private {
    use serde::de::DeserializeSeed;
    use serde::{Deserializer, Serialize, Serializer};

    use crate::{DeserializeState, SerializeState};

    pub struct SerializeRef<'state, T: ?Sized, State: ?Sized> {
        value: &'state T,
        state: &'state State,
    }

    impl<'state, T, State> SerializeRef<'state, T, State>
    where
        T: ?Sized,
        State: ?Sized,
    {
        pub fn new(value: &'state T, state: &'state State) -> Self {
            Self { value, state }
        }
    }

    impl<'state, T, State> Serialize for SerializeRef<'state, T, State>
    where
        T: SerializeState<State> + ?Sized,
        State: ?Sized,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            self.value.serialize_state(self.state, serializer)
        }
    }

    pub fn wrap_serialize<'state, T, State>(
        value: &'state T,
        state: &'state State,
    ) -> SerializeRef<'state, T, State>
    where
        T: SerializeState<State> + ?Sized,
        State: ?Sized,
    {
        SerializeRef::new(value, state)
    }

    pub struct DeserializeStateSeed<'state, T, State: ?Sized> {
        state: &'state State,
        _marker: core::marker::PhantomData<T>,
    }

    impl<T, State: ?Sized> Copy for DeserializeStateSeed<'_, T, State> {}
    impl<T, State: ?Sized> Clone for DeserializeStateSeed<'_, T, State> {
        fn clone(&self) -> Self {
            Self {
                state: self.state,
                _marker: self._marker,
            }
        }
    }

    impl<'state, T, State: ?Sized> DeserializeStateSeed<'state, T, State> {
        pub fn new(state: &'state State) -> Self {
            Self {
                state,
                _marker: core::marker::PhantomData,
            }
        }
    }

    impl<'de, 'state, T, State> DeserializeSeed<'de> for DeserializeStateSeed<'state, T, State>
    where
        T: DeserializeState<'de, State>,
        State: ?Sized,
    {
        type Value = T;

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            T::deserialize_state(self.state, deserializer)
        }
    }

    pub fn wrap_deserialize_seed<'state, T, State: ?Sized>(
        state: &'state State,
    ) -> DeserializeStateSeed<'state, T, State> {
        DeserializeStateSeed::new(state)
    }
}
impl<State: ?Sized, T> SerializeState<State> for Vec<T>
where
    T: SerializeState<State>,
{
    fn serialize_state<S>(&self, state: &State, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for value in self {
            seq.serialize_element(&crate::__private::wrap_serialize(value, state))?;
        }
        seq.end()
    }
}

impl<'de, State: ?Sized, T> DeserializeState<'de, State> for Vec<T>
where
    T: DeserializeState<'de, State>,
{
    fn deserialize_state<D>(state: &State, deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct VecVisitor<'state, State: ?Sized, T> {
            state: &'state State,
            marker: PhantomData<T>,
        }

        impl<'de, 'state, State: ?Sized, T> serde::de::Visitor<'de> for VecVisitor<'state, State, T>
        where
            T: DeserializeState<'de, State>,
        {
            type Value = Vec<T>;

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element_seed(
                    crate::__private::wrap_deserialize_seed::<T, State>(self.state),
                )? {
                    values.push(value);
                }
                Ok(values)
            }
        }

        deserializer.deserialize_seq(VecVisitor {
            state,
            marker: PhantomData,
        })
    }
}

impl<State: ?Sized, T> SerializeState<State> for Option<T>
where
    T: SerializeState<State>,
{
    fn serialize_state<S>(&self, state: &State, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Some(value) => {
                serializer.serialize_some(&crate::__private::wrap_serialize(value, state))
            }
            None => serializer.serialize_none(),
        }
    }
}

impl<'de, State: ?Sized, T> DeserializeState<'de, State> for Option<T>
where
    T: DeserializeState<'de, State>,
{
    fn deserialize_state<D>(state: &State, deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OptionVisitor<'state, State: ?Sized, T> {
            state: &'state State,
            marker: PhantomData<T>,
        }

        impl<'de, 'state, State: ?Sized, T> serde::de::Visitor<'de> for OptionVisitor<'state, State, T>
        where
            T: DeserializeState<'de, State>,
        {
            type Value = Option<T>;

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("an optional value")
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let seed = crate::__private::wrap_deserialize_seed::<T, State>(self.state);
                let value = serde::de::DeserializeSeed::deserialize(seed, deserializer)?;
                Ok(Some(value))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(None)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(None)
            }
        }

        deserializer.deserialize_option(OptionVisitor {
            state,
            marker: PhantomData,
        })
    }
}
impl<State: ?Sized, A, B> SerializeState<State> for (A, B)
where
    A: SerializeState<State>,
    B: SerializeState<State>,
{
    fn serialize_state<S>(&self, state: &State, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_tuple(2)?;
        seq.serialize_element(&crate::__private::wrap_serialize(&self.0, state))?;
        seq.serialize_element(&crate::__private::wrap_serialize(&self.1, state))?;
        seq.end()
    }
}

impl<'de, State: ?Sized, A, B> DeserializeState<'de, State> for (A, B)
where
    A: DeserializeState<'de, State>,
    B: DeserializeState<'de, State>,
{
    fn deserialize_state<D>(state: &State, deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Tuple2Visitor<'state, State: ?Sized, A, B> {
            state: &'state State,
            marker: PhantomData<(A, B)>,
        }

        impl<'de, 'state, State: ?Sized, A, B> serde::de::Visitor<'de>
            for Tuple2Visitor<'state, State, A, B>
        where
            A: DeserializeState<'de, State>,
            B: DeserializeState<'de, State>,
        {
            type Value = (A, B);

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str("a tuple of length 2")
            }

            fn visit_seq<Seq>(self, mut seq: Seq) -> Result<Self::Value, Seq::Error>
            where
                Seq: serde::de::SeqAccess<'de>,
            {
                let first = seq
                    .next_element_seed(crate::__private::wrap_deserialize_seed::<A, State>(
                        self.state,
                    ))?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let second = seq
                    .next_element_seed(crate::__private::wrap_deserialize_seed::<B, State>(
                        self.state,
                    ))?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                Ok((first, second))
            }
        }

        deserializer.deserialize_tuple(
            2,
            Tuple2Visitor {
                state,
                marker: PhantomData,
            },
        )
    }
}
