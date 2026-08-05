use std::{future::Future, marker::PhantomData, pin::Pin};

use zvariant::{DynamicType, OwnedValue, Type, Value};

use crate::{MethodError, MethodResult};

pub(super) type PropertyFuture<'a, T> = Pin<Box<dyn Future<Output = MethodResult<T>> + 'a>>;

pub(super) trait Property {
    fn signature(&self) -> &str;
    fn writable(&self) -> bool;
    fn change_mode(&self) -> PropertyChangeMode;
    fn get(&mut self) -> PropertyFuture<'_, OwnedValue>;
    fn set(&mut self, value: OwnedValue) -> PropertyFuture<'_, PropertyUpdate>;
}

/// How a property advertises and emits `PropertiesChanged` notifications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropertyChangeMode {
    Value,
    Invalidates,
    Const,
    Silent,
}

impl PropertyChangeMode {
    pub(super) const fn annotation(self) -> &'static str {
        match self {
            Self::Value => "true",
            Self::Invalidates => "invalidates",
            Self::Const => "const",
            Self::Silent => "false",
        }
    }
}

pub(super) enum PropertyUpdate {
    Value(OwnedValue),
    Invalidated,
    None,
}

pub(super) struct ReadOnlyProperty<G, T, F> {
    getter: G,
    signature: String,
    change_mode: PropertyChangeMode,
    marker: PhantomData<fn() -> (T, F)>,
}

impl<G, T, F> ReadOnlyProperty<G, T, F>
where
    T: DynamicType + Into<Value<'static>>,
{
    pub(super) fn new(getter: G, change_mode: PropertyChangeMode) -> Self
    where
        T: Type,
    {
        Self {
            getter,
            signature: T::SIGNATURE.to_string(),
            change_mode,
            marker: PhantomData,
        }
    }
}

impl<G, T, F> Property for ReadOnlyProperty<G, T, F>
where
    G: FnMut() -> F,
    T: DynamicType + Into<Value<'static>>,
    F: Future<Output = MethodResult<T>> + 'static,
{
    fn signature(&self) -> &str {
        &self.signature
    }

    fn writable(&self) -> bool {
        false
    }

    fn change_mode(&self) -> PropertyChangeMode {
        self.change_mode
    }

    fn get(&mut self) -> PropertyFuture<'_, OwnedValue> {
        let future = (self.getter)();
        Box::pin(async move { future.await.and_then(into_owned_value) })
    }

    fn set(&mut self, _value: OwnedValue) -> PropertyFuture<'_, PropertyUpdate> {
        Box::pin(async { Err(MethodError::property_read_only("property is read-only")) })
    }
}

pub(super) struct ReadWriteProperty<G, S, T, GF, SF> {
    getter: G,
    setter: S,
    signature: String,
    change_mode: PropertyChangeMode,
    value: PhantomData<fn() -> T>,
    getter_future: PhantomData<fn() -> GF>,
    setter_future: PhantomData<fn() -> SF>,
}

impl<G, S, T, GF, SF> ReadWriteProperty<G, S, T, GF, SF>
where
    T: DynamicType + Into<Value<'static>>,
{
    pub(super) fn new(getter: G, setter: S, change_mode: PropertyChangeMode) -> Self
    where
        T: Type,
    {
        Self {
            getter,
            setter,
            signature: T::SIGNATURE.to_string(),
            change_mode,
            value: PhantomData,
            getter_future: PhantomData,
            setter_future: PhantomData,
        }
    }
}

impl<G, S, T, GF, SF, E> Property for ReadWriteProperty<G, S, T, GF, SF>
where
    G: FnMut() -> GF,
    S: FnMut(T) -> SF,
    T: DynamicType + Into<Value<'static>> + TryFrom<OwnedValue, Error = E>,
    GF: Future<Output = MethodResult<T>> + 'static,
    SF: Future<Output = MethodResult<()>> + 'static,
    E: std::fmt::Display,
{
    fn signature(&self) -> &str {
        &self.signature
    }

    fn writable(&self) -> bool {
        true
    }

    fn change_mode(&self) -> PropertyChangeMode {
        self.change_mode
    }

    fn get(&mut self) -> PropertyFuture<'_, OwnedValue> {
        let future = (self.getter)();
        Box::pin(async move { future.await.and_then(into_owned_value) })
    }

    fn set(&mut self, value: OwnedValue) -> PropertyFuture<'_, PropertyUpdate> {
        match T::try_from(value) {
            Ok(value) => {
                let setter = (self.setter)(value);
                match self.change_mode {
                    PropertyChangeMode::Value => {
                        let getter = &mut self.getter;
                        Box::pin(async move {
                            setter.await?;
                            let value = (getter)().await?;
                            into_owned_value(value).map(PropertyUpdate::Value)
                        })
                    }
                    PropertyChangeMode::Invalidates => Box::pin(async move {
                        setter.await?;
                        Ok(PropertyUpdate::Invalidated)
                    }),
                    PropertyChangeMode::Const | PropertyChangeMode::Silent => {
                        Box::pin(async move {
                            setter.await?;
                            Ok(PropertyUpdate::None)
                        })
                    }
                }
            }
            Err(error) => {
                let error = MethodError::invalid_args(error);
                Box::pin(async move { Err(error) })
            }
        }
    }
}

fn into_owned_value<T>(value: T) -> MethodResult<OwnedValue>
where
    T: DynamicType + Into<Value<'static>>,
{
    Value::new(value)
        .try_into_owned()
        .map_err(|error| MethodError::failed(error.to_string()))
}
