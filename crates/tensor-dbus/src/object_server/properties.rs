use zvariant::OwnedValue;

use crate::{Connection, Error, MethodCall, MethodError, MethodResult, Result};

use super::{Interface, Object, PROPERTIES, PropertyChangeMode, PropertyMap, PropertyUpdate};

pub(super) async fn get_property(
    object: &mut Object,
    call: &MethodCall,
) -> MethodResult<OwnedValue> {
    let (interface, name): (String, String) = call.body()?;
    lookup_property(object, &interface, &name)?.get().await
}

pub(super) async fn get_all_properties(
    object: &mut Object,
    call: &MethodCall,
) -> MethodResult<PropertyMap> {
    let interface: String = call.body()?;
    match object.interfaces.get_mut(&interface) {
        Some(interface) => collect_properties(interface).await,
        None => Ok(PropertyMap::new()),
    }
}

pub(super) async fn set_property(
    object: &mut Object,
    call: &MethodCall,
) -> MethodResult<(String, String, PropertyUpdate)> {
    let (interface, name, value): (String, String, OwnedValue) = call.body()?;
    let update = lookup_property(object, &interface, &name)?
        .set(value)
        .await?;
    Ok((interface, name, update))
}

pub(super) async fn emit_property_update(
    connection: &mut Connection,
    path: &str,
    update: (String, String, PropertyUpdate),
) -> Result<()> {
    let (interface, name, update) = update;
    match update {
        PropertyUpdate::Value(value) => {
            let mut changed = PropertyMap::with_capacity(1);
            changed.insert(name, value);
            emit_property_changes(connection, path, &interface, changed, Vec::new()).await
        }
        PropertyUpdate::Invalidated => {
            emit_property_changes(connection, path, &interface, PropertyMap::new(), vec![name])
                .await
        }
        PropertyUpdate::None => Ok(()),
    }
}

pub(super) async fn emit_property_changes(
    connection: &mut Connection,
    path: &str,
    interface: &str,
    changed: PropertyMap,
    invalidated: Vec<String>,
) -> Result<()> {
    if changed.is_empty() && invalidated.is_empty() {
        return Ok(());
    }
    connection
        .emit_signal(
            path,
            PROPERTIES,
            "PropertiesChanged",
            &(interface, changed, invalidated),
        )
        .await
}

pub(super) async fn collect_properties(interface: &mut Interface) -> MethodResult<PropertyMap> {
    let mut properties = PropertyMap::with_capacity(interface.properties.len());
    for (name, property) in &mut interface.properties {
        properties.insert(name.clone(), property.get().await?);
    }
    Ok(properties)
}

pub(super) async fn collect_interfaces(object: &mut Object) -> MethodResult<super::InterfaceMap> {
    let mut interfaces = super::InterfaceMap::with_capacity(object.interfaces.len());
    for (name, interface) in &mut object.interfaces {
        interfaces.insert(name.clone(), collect_properties(interface).await?);
    }
    Ok(interfaces)
}

pub(super) async fn collect_changes(
    object: &mut Object,
    interface: &str,
    names: &[&str],
) -> Result<(PropertyMap, Vec<String>)> {
    let interface_entry =
        object
            .interfaces
            .get_mut(interface)
            .ok_or_else(|| Error::InvalidName {
                kind: "registered interface name",
                value: interface.to_owned(),
            })?;
    let mut values = PropertyMap::with_capacity(names.len());
    let mut invalidated = Vec::new();
    for name in names {
        let property =
            interface_entry
                .properties
                .get_mut(*name)
                .ok_or_else(|| Error::InvalidName {
                    kind: "registered property name",
                    value: (*name).to_owned(),
                })?;
        match property.change_mode() {
            PropertyChangeMode::Value => {
                values.insert(
                    (*name).to_owned(),
                    property.get().await.map_err(Error::Service)?,
                );
            }
            PropertyChangeMode::Invalidates => invalidated.push((*name).to_owned()),
            PropertyChangeMode::Const | PropertyChangeMode::Silent => {}
        }
    }
    Ok((values, invalidated))
}

fn lookup_property<'a>(
    object: &'a mut Object,
    interface: &str,
    name: &str,
) -> MethodResult<&'a mut Box<dyn super::Property>> {
    let interface = object
        .interfaces
        .get_mut(interface)
        .ok_or_else(|| MethodError::unknown_interface(format!("unknown interface {interface}")))?;
    interface
        .properties
        .get_mut(name)
        .ok_or_else(|| MethodError::unknown_property(format!("unknown property {name}")))
}
