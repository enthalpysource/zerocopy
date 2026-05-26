This crate provides traits and derive macros for stateful serde (de)serialization.

This is based on [`serde_state`](https://github.com/Marwes/serde_state) but rewritten from scratch
to use perfect derive to avoid the need for explicit state annotations. Typical usage looks like:

```rust
#[derive(Default)]
struct Recorder {
    serialized: Cell<usize>,
    deserialized: Cell<usize>,
}

#[derive(Clone, Debug, PartialEq)]
struct CounterValue(u32);

// This type requires a specific state during serialization/deserialization.
impl SerializeState<Recorder> for CounterValue {
    fn serialize_state<S>(&self, recorder: &Recorder, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        recorder.serialized.set(recorder.serialized.get() + 1);
        serializer.serialize_u32(self.0)
    }
}
impl<'de> DeserializeState<'de, Recorder> for CounterValue {
    fn deserialize_state<D>(recorder: &Recorder, deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        recorder.deserialized.set(recorder.deserialized.get() + 1);
        let value = u32::deserialize(deserializer)?;
        Ok(CounterValue(value))
    }
}

// This type just passes whatever state through.
#[derive(SerializeState, DeserializeState, Debug, PartialEq)]
struct Example {
    first: CounterValue,
    second: CounterValue,
    #[stateless] // use normal serde impls for this
    third: usize,
}

let value = Example {
    first: CounterValue(1),
    second: CounterValue(2),
    third: 42,
};
let state = Recorder::default();
let mut buffer = Vec::new();
{
    let mut serializer = serde_json::Serializer::new(&mut buffer);
    value
        .serialize_state(&state, &mut serializer)
        .expect("serialization should succeed");
}
assert_eq!(state.serialized.get(), 2);
```

### Stateless or stateful overrides

By default every type derived with `SerializeState`/`DeserializeState` is stateful, meaning
every field is serialized through the stateful traits. You can opt out by using
`#[serde_state(stateless)]` on the container, on a specific enum variant, or on individual
fields. The attribute can also be flipped back with `#[serde_state(stateful)]` when you want to
override a stateless container.

```rust
#[derive(SerializeState, DeserializeState)]
struct Mixed {
    #[serde_state(stateless)]
    plain: u32,
    counter: CounterValue,
}

#[derive(SerializeState, DeserializeState)]
enum VariantModes {
    #[serde_state(stateless)]
    Plain(u32),
    #[serde_state(stateless)]
    Struct { value: u32 },
    #[serde_state(stateless)]
    WithOverride { #[serde_state(stateful)] counter: CounterValue },
}
```

### Renaming and skipping fields

The standard `#[serde(rename = "...")]` and `#[serde(skip)]` field attributes are
supported. Renamed fields use the provided key during serialization/deserialization, while
skipped fields are omitted and initialized via `Default::default()` when deserializing.

```rust
#[derive(SerializeState, DeserializeState)]
struct Renamed {
    #[serde(rename = "external")]
    counter: CounterValue,
    #[serde(skip)]
    cached: PlainValue,
}
```

### Recursive structures

Because this uses perfect derives, the derive macro causes trait errors on recursive types.
To solve this, opt out of inference by specifying the state explicitly with
`#[serde_state(state = MyState)]` or `#[serde_state(state_implements = HasMyState)]`.
When the state is set manually
the derive falls back to placing bounds on each type parameter instead of every field.

```rust
#[derive(SerializeState, DeserializeState)]
#[serde_state(state = Recorder)]
enum CounterList {
    Nil,
    Cons(CounterValue, Box<CounterList>),
}
```
