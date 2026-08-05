use std::{borrow::Cow, collections::HashMap, future::Future, marker::PhantomData};

use serde::{Serialize, de::DeserializeOwned};
use zvariant::{DynamicType, OwnedObjectPath, OwnedValue, Type, Value};

use crate::{
    Connection, Error, MachineId, Message, MessageKind, MethodCall, MethodError, MethodResult,
    Result,
    name::{validate_interface_name, validate_member_name},
    reply_method, reply_method_error, reply_method_result,
};

const INTROSPECTABLE: &str = "org.freedesktop.DBus.Introspectable";
const PEER: &str = "org.freedesktop.DBus.Peer";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

mod handler;
mod introspection;
mod lifecycle;
mod properties;
mod property;

pub use handler::MethodContext;
use handler::{ConnectionHandler, ContextHandler, ErasedHandler, TypedHandler};
use introspection::body_signatures;
use lifecycle::{is_descendant, validate_manager_path, validate_manager_relationship};
use properties::{
    collect_changes, collect_interfaces, emit_property_changes, emit_property_update,
    get_all_properties, get_property, set_property,
};
pub use property::PropertyChangeMode;
use property::{Property, PropertyUpdate, ReadOnlyProperty, ReadWriteProperty};

type PropertyMap = HashMap<String, OwnedValue>;
type InterfaceMap = HashMap<String, PropertyMap>;
type ManagedObjects = HashMap<OwnedObjectPath, InterfaceMap>;

struct Method {
    input: Vec<String>,
    output: Vec<String>,
    handler: Box<dyn ErasedHandler>,
}

struct Signal {
    arguments: Vec<String>,
}

#[derive(Default)]
struct Interface {
    methods: HashMap<String, Method>,
    properties: HashMap<String, Box<dyn Property>>,
    signals: HashMap<String, Signal>,
}

#[derive(Default)]
struct Object {
    interfaces: HashMap<String, Interface>,
    introspection: String,
    object_manager: bool,
    children: Vec<String>,
    registered: bool,
}

/// A caller-driven object and method router for bus and peer connections.
///
/// Registration is the allocation-heavy cold path. Dispatch performs hashed
/// path, interface, and member lookups and awaits handlers directly on the
/// caller's Compio runtime without spawning tasks or requiring `Send`.
#[derive(Default)]
pub struct ObjectServer {
    objects: HashMap<String, Object>,
    machine_id: Option<MachineId>,
}

impl ObjectServer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_machine_id(machine_id: MachineId) -> Self {
        Self {
            machine_id: Some(machine_id),
            ..Self::default()
        }
    }

    pub fn set_machine_id(&mut self, machine_id: MachineId) {
        self.machine_id = Some(machine_id);
        self.refresh_introspection();
    }

    pub fn register<A, R, F, H>(
        &mut self,
        path: &str,
        interface: &str,
        member: &str,
        handler: H,
    ) -> Result<()>
    where
        H: FnMut(A) -> F + 'static,
        A: DeserializeOwned + Type + 'static,
        R: Serialize + DynamicType + Type + 'static,
        F: Future<Output = MethodResult<R>> + 'static,
    {
        self.insert::<A, R>(
            path,
            interface,
            member,
            Box::new(TypedHandler {
                handler,
                marker: PhantomData,
            }),
        )
    }

    pub fn register_with_context<A, R, F, H>(
        &mut self,
        path: &str,
        interface: &str,
        member: &str,
        handler: H,
    ) -> Result<()>
    where
        H: FnMut(MethodContext, A) -> F + 'static,
        A: DeserializeOwned + Type + 'static,
        R: Serialize + DynamicType + Type + 'static,
        F: Future<Output = MethodResult<R>> + 'static,
    {
        self.insert::<A, R>(
            path,
            interface,
            member,
            Box::new(ContextHandler {
                handler,
                marker: PhantomData,
            }),
        )
    }

    /// Registers a typed async handler that can drive the active connection.
    ///
    /// The handler may emit signals or perform nested calls before returning
    /// its method result. It runs inline on the caller's Compio runtime and
    /// need not be `Send`.
    pub fn register_with_connection<A, R, H>(
        &mut self,
        path: &str,
        interface: &str,
        member: &str,
        handler: H,
    ) -> Result<()>
    where
        H: for<'a> AsyncFnMut(&'a mut Connection, MethodContext, A) -> MethodResult<R> + 'static,
        A: DeserializeOwned + Type + 'static,
        R: Serialize + DynamicType + Type + 'static,
    {
        self.insert::<A, R>(
            path,
            interface,
            member,
            Box::new(ConnectionHandler {
                handler,
                marker: PhantomData,
            }),
        )
    }

    pub fn contains_object(&self, path: &str) -> bool {
        self.objects
            .get(path)
            .is_some_and(|object| object.registered)
    }

    /// Returns whether a path is present in the introspection tree.
    ///
    /// This includes virtual ancestors synthesized for registered descendants.
    pub fn contains_introspection_path(&self, path: &str) -> bool {
        self.objects.contains_key(path)
    }

    pub fn unregister_interface(&mut self, path: &str, interface: &str) -> Result<bool> {
        validate_path(path)?;
        validate_interface_name(interface, "interface name")?;
        let Some(object) = self
            .objects
            .get_mut(path)
            .filter(|object| object.registered)
        else {
            return Ok(false);
        };
        let removed = object.interfaces.remove(interface).is_some();
        let remove_object = removed && object.interfaces.is_empty() && !object.object_manager;
        if removed {
            if remove_object {
                self.objects.remove(path);
            }
            self.refresh_introspection();
        }
        Ok(removed)
    }

    pub fn unregister_object(&mut self, path: &str) -> Result<Vec<String>> {
        validate_path(path)?;
        let Some(object) = self.objects.get(path).filter(|object| object.registered) else {
            return Ok(Vec::new());
        };
        let mut interfaces: Vec<_> = object.interfaces.keys().cloned().collect();
        interfaces.sort_unstable();
        self.objects.remove(path);
        self.refresh_introspection();
        Ok(interfaces)
    }

    pub fn register_read_only_property<T, F, G>(
        &mut self,
        path: &str,
        interface: &str,
        name: &str,
        change_mode: PropertyChangeMode,
        getter: G,
    ) -> Result<()>
    where
        G: FnMut() -> F + 'static,
        T: DynamicType + Into<Value<'static>> + Type + 'static,
        F: Future<Output = MethodResult<T>> + 'static,
    {
        self.insert_property(
            path,
            interface,
            name,
            Box::new(ReadOnlyProperty::<G, T, F>::new(getter, change_mode)),
        )
    }

    pub fn register_property<T, GF, SF, G, S, E>(
        &mut self,
        path: &str,
        interface: &str,
        name: &str,
        change_mode: PropertyChangeMode,
        getter: G,
        setter: S,
    ) -> Result<()>
    where
        G: FnMut() -> GF + 'static,
        S: FnMut(T) -> SF + 'static,
        T: DynamicType + Into<Value<'static>> + TryFrom<OwnedValue, Error = E> + Type + 'static,
        GF: Future<Output = MethodResult<T>> + 'static,
        SF: Future<Output = MethodResult<()>> + 'static,
        E: std::fmt::Display + 'static,
    {
        self.insert_property(
            path,
            interface,
            name,
            Box::new(ReadWriteProperty::<G, S, T, GF, SF>::new(
                getter,
                setter,
                change_mode,
            )),
        )
    }

    pub fn enable_object_manager(&mut self, path: &str) -> Result<()> {
        validate_path(path)?;
        let object = self.objects.entry(path.to_owned()).or_default();
        object.registered = true;
        object.object_manager = true;
        self.refresh_introspection();
        Ok(())
    }

    fn insert<A: Type, R: Type>(
        &mut self,
        path: &str,
        interface: &str,
        member: &str,
        handler: Box<dyn ErasedHandler>,
    ) -> Result<()> {
        validate_path(path)?;
        validate_interface_name(interface, "interface name")?;
        validate_member_name(member, "member name")?;
        if is_standard_interface(interface) {
            return Err(Error::ReservedInterface(interface.to_owned()));
        }

        let object = self.objects.entry(path.to_owned()).or_default();
        object.registered = true;
        let methods = &mut object
            .interfaces
            .entry(interface.to_owned())
            .or_default()
            .methods;
        if methods.contains_key(member) {
            return Err(Error::DuplicateMethod {
                path: path.to_owned(),
                interface: interface.to_owned(),
                member: member.to_owned(),
            });
        }
        methods.insert(
            member.to_owned(),
            Method {
                input: body_signatures::<A>(),
                output: body_signatures::<R>(),
                handler,
            },
        );
        self.refresh_introspection();
        Ok(())
    }

    fn insert_property(
        &mut self,
        path: &str,
        interface: &str,
        name: &str,
        property: Box<dyn Property>,
    ) -> Result<()> {
        validate_path(path)?;
        validate_interface_name(interface, "interface name")?;
        validate_member_name(name, "property name")?;
        if is_standard_interface(interface) {
            return Err(Error::ReservedInterface(interface.to_owned()));
        }
        let object = self.objects.entry(path.to_owned()).or_default();
        object.registered = true;
        let properties = &mut object
            .interfaces
            .entry(interface.to_owned())
            .or_default()
            .properties;
        if properties.contains_key(name) {
            return Err(Error::DuplicateProperty {
                path: path.to_owned(),
                interface: interface.to_owned(),
                property: name.to_owned(),
            });
        }
        properties.insert(name.to_owned(), property);
        self.refresh_introspection();
        Ok(())
    }

    pub async fn dispatch(&mut self, connection: &mut Connection, call: &MethodCall) -> Result<()> {
        let machine_id = self.machine_id;
        let object_manager_call = call.member() == "GetManagedObjects"
            && match call.interface() {
                Some(interface) => interface == OBJECT_MANAGER,
                None => self
                    .objects
                    .get(call.path())
                    .is_some_and(|object| object.object_manager),
            };
        if object_manager_call {
            return self.dispatch_object_manager(connection, call).await;
        }
        let Some(object) = self.objects.get_mut(call.path()) else {
            return reply_method_error(
                connection,
                call,
                MethodError::unknown_object(format!("unknown object {}", call.path())),
            )
            .await;
        };

        let interface = match call.interface() {
            Some(interface) => Cow::Borrowed(interface),
            None => match unique_interface_for_member(object, call.member()) {
                Some(interface) => Cow::Owned(interface),
                None => {
                    return reply_method_error(
                        connection,
                        call,
                        MethodError::unknown_method(format!(
                            "method {} requires an unambiguous interface",
                            call.member()
                        )),
                    )
                    .await;
                }
            },
        };
        if matches!(interface.as_ref(), INTROSPECTABLE | PEER)
            || interface.as_ref() == PROPERTIES && has_properties(object)
        {
            return standard_call(object, machine_id, interface.as_ref(), call, connection).await;
        }
        if interface.as_ref() == PROPERTIES {
            return reply_method_error(
                connection,
                call,
                MethodError::unknown_interface(format!("unknown interface {PROPERTIES}")),
            )
            .await;
        }
        if interface.as_ref() == OBJECT_MANAGER {
            let error = if object.object_manager {
                MethodError::unknown_method(format!("unknown method {}", call.member()))
            } else {
                MethodError::unknown_interface(format!("unknown interface {OBJECT_MANAGER}"))
            };
            return reply_method_error(connection, call, error).await;
        }
        let Some(interface_entry) = object.interfaces.get_mut(interface.as_ref()) else {
            return reply_method_error(
                connection,
                call,
                MethodError::unknown_interface(format!("unknown interface {interface}")),
            )
            .await;
        };
        let Some(method) = interface_entry.methods.get_mut(call.member()) else {
            return reply_method_error(
                connection,
                call,
                MethodError::unknown_method(format!("unknown method {}", call.member())),
            )
            .await;
        };
        method.handler.call(connection, call).await
    }

    async fn dispatch_object_manager(
        &mut self,
        connection: &mut Connection,
        call: &MethodCall,
    ) -> Result<()> {
        let Some(root) = self.objects.get(call.path()) else {
            return reply_method_error(
                connection,
                call,
                MethodError::unknown_object(format!("unknown object {}", call.path())),
            )
            .await;
        };
        if !root.object_manager {
            return reply_method_error(
                connection,
                call,
                MethodError::unknown_interface(format!("unknown interface {OBJECT_MANAGER}")),
            )
            .await;
        }
        if let Err(error) = call.body::<()>() {
            return reply_method_error(connection, call, error).await;
        }
        let root_path = call.path().to_owned();
        let mut managed = ManagedObjects::new();
        for (path, object) in &mut self.objects {
            if !object.registered || !is_descendant(&root_path, path) {
                continue;
            }
            let interfaces = collect_interfaces(object).await;
            match interfaces {
                Ok(interfaces) => {
                    managed.insert(
                        OwnedObjectPath::try_from(path.as_str())
                            .expect("registered object paths are validated"),
                        interfaces,
                    );
                }
                Err(error) => return reply_method_error(connection, call, error).await,
            }
        }
        reply_method(connection, call, &managed).await
    }

    pub async fn emit_properties_changed(
        &mut self,
        connection: &mut Connection,
        path: &str,
        interface: &str,
        names: &[&str],
    ) -> Result<()> {
        let object = self
            .objects
            .get_mut(path)
            .filter(|object| object.registered)
            .ok_or_else(|| Error::InvalidName {
                kind: "registered object path",
                value: path.to_owned(),
            })?;
        let (changed, invalidated) = collect_changes(object, interface, names).await?;
        emit_property_changes(connection, path, interface, changed, invalidated).await
    }

    pub async fn emit_interfaces_added(
        &mut self,
        connection: &mut Connection,
        manager_path: &str,
        object_path: &str,
    ) -> Result<()> {
        validate_manager_relationship(self, manager_path, object_path)?;
        let interfaces = collect_interfaces(
            self.objects
                .get_mut(object_path)
                .expect("relationship validation checks the object"),
        )
        .await
        .map_err(Error::Service)?;
        let path = OwnedObjectPath::try_from(object_path).expect("registered path is validated");
        connection
            .emit_signal(
                manager_path,
                OBJECT_MANAGER,
                "InterfacesAdded",
                &(path, interfaces),
            )
            .await
    }

    pub async fn emit_interfaces_removed(
        &self,
        connection: &mut Connection,
        manager_path: &str,
        object_path: &str,
        interfaces: &[&str],
    ) -> Result<()> {
        validate_manager_path(self, manager_path)?;
        if !is_descendant(manager_path, object_path) {
            return Err(Error::InvalidName {
                kind: "managed object path",
                value: object_path.to_owned(),
            });
        }
        validate_path(object_path)?;
        for interface in interfaces {
            validate_interface_name(interface, "interface name")?;
        }
        let path = OwnedObjectPath::try_from(object_path).expect("registered path is validated");
        connection
            .emit_signal(
                manager_path,
                OBJECT_MANAGER,
                "InterfacesRemoved",
                &(path, interfaces),
            )
            .await
    }

    /// Receives one message and dispatches it when it is a method call.
    ///
    /// Signals and replies are returned unchanged so a caller combining client
    /// and service roles on one connection retains full routing control.
    pub async fn serve_next(&mut self, connection: &mut Connection) -> Result<Option<Message>> {
        let message = connection.receive().await?;
        if message.kind() != MessageKind::MethodCall {
            return Ok(Some(message));
        }
        let call = MethodCall::new(message).expect("message kind was checked");
        self.dispatch(connection, &call).await?;
        Ok(None)
    }
}

async fn standard_call(
    object: &mut Object,
    machine_id: Option<MachineId>,
    interface: &str,
    call: &MethodCall,
    connection: &mut Connection,
) -> Result<()> {
    match (interface, call.member()) {
        (INTROSPECTABLE, "Introspect") => {
            let body = call.body::<()>();
            match body {
                Ok(()) => reply_method(connection, call, &object.introspection).await?,
                Err(error) => reply_method_error(connection, call, error).await?,
            }
        }
        (PEER, "Ping") => {
            let body = call.body::<()>();
            reply_method_result(connection, call, body).await?;
        }
        (PEER, "GetMachineId") => {
            let body = call.body::<()>();
            let result = body.and_then(|()| {
                machine_id
                    .map(|id| id.to_string())
                    .ok_or_else(|| MethodError::failed("the service has no configured machine ID"))
            });
            reply_method_result(connection, call, result).await?;
        }
        (PROPERTIES, "Get") => {
            let result = get_property(object, call).await;
            reply_method_result(connection, call, result).await?;
        }
        (PROPERTIES, "GetAll") => {
            let result = get_all_properties(object, call).await;
            reply_method_result(connection, call, result).await?;
        }
        (PROPERTIES, "Set") => match set_property(object, call).await {
            Ok(update) => {
                reply_method(connection, call, &()).await?;
                emit_property_update(connection, call.path(), update).await?;
            }
            Err(error) => reply_method_error(connection, call, error).await?,
        },
        _ => {
            reply_method_error(
                connection,
                call,
                MethodError::unknown_method(format!("unknown method {}", call.member())),
            )
            .await?;
        }
    }
    Ok(())
}

fn unique_interface_for_member(object: &Object, member: &str) -> Option<String> {
    let standard = match member {
        "Introspect" => Some(INTROSPECTABLE),
        "Ping" | "GetMachineId" => Some(PEER),
        "Get" | "GetAll" | "Set" if has_properties(object) => Some(PROPERTIES),
        "GetManagedObjects" if object.object_manager => Some(OBJECT_MANAGER),
        _ => None,
    };
    let mut matches = object
        .interfaces
        .iter()
        .filter(|(_, interface)| interface.methods.contains_key(member));
    match (standard, matches.next(), matches.next()) {
        (Some(name), None, None) => Some(name.to_owned()),
        (None, Some((name, _)), None) => Some(name.clone()),
        _ => None,
    }
}

fn has_properties(object: &Object) -> bool {
    object
        .interfaces
        .values()
        .any(|interface| !interface.properties.is_empty())
}

fn is_standard_interface(interface: &str) -> bool {
    matches!(
        interface,
        INTROSPECTABLE | PEER | PROPERTIES | OBJECT_MANAGER
    )
}

fn validate_path(path: &str) -> Result<()> {
    zvariant::ObjectPath::try_from(path).map_err(|_| Error::InvalidName {
        kind: "object path",
        value: path.to_owned(),
    })?;
    Ok(())
}
