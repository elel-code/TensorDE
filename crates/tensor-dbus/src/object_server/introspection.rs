use zvariant::{Signature, Type};

use crate::{
    Error, Result,
    name::{validate_interface_name, validate_member_name},
};

use super::{Object, ObjectServer, Signal, has_properties, is_standard_interface, validate_path};

const PROPERTIES_XML: &str = "  <interface name=\"org.freedesktop.DBus.Properties\">\n    <method name=\"Get\"><arg type=\"s\" direction=\"in\"/><arg type=\"s\" direction=\"in\"/><arg type=\"v\" direction=\"out\"/></method>\n    <method name=\"GetAll\"><arg type=\"s\" direction=\"in\"/><arg type=\"a{sv}\" direction=\"out\"/></method>\n    <method name=\"Set\"><arg type=\"s\" direction=\"in\"/><arg type=\"s\" direction=\"in\"/><arg type=\"v\" direction=\"in\"/></method>\n    <signal name=\"PropertiesChanged\"><arg type=\"s\"/><arg type=\"a{sv}\"/><arg type=\"as\"/></signal>\n  </interface>\n";
const OBJECT_MANAGER_XML: &str = "  <interface name=\"org.freedesktop.DBus.ObjectManager\">\n    <method name=\"GetManagedObjects\"><arg type=\"a{oa{sa{sv}}}\" direction=\"out\"/></method>\n    <signal name=\"InterfacesAdded\"><arg type=\"o\"/><arg type=\"a{sa{sv}}\"/></signal>\n    <signal name=\"InterfacesRemoved\"><arg type=\"o\"/><arg type=\"as\"/></signal>\n  </interface>\n";

impl Object {
    pub(super) fn rebuild_introspection(&mut self, has_machine_id: bool) {
        let mut interfaces: Vec<_> = self.interfaces.iter().collect();
        interfaces.sort_unstable_by_key(|(name, _)| name.as_str());
        let mut xml = String::from("<node>\n");
        for (name, interface) in interfaces {
            xml.push_str("  <interface name=\"");
            xml.push_str(name);
            xml.push_str("\">\n");
            let mut methods: Vec<_> = interface.methods.iter().collect();
            methods.sort_unstable_by_key(|(name, _)| name.as_str());
            for (name, method) in methods {
                xml.push_str("    <method name=\"");
                xml.push_str(name);
                xml.push_str("\">\n");
                push_arguments(&mut xml, &method.input, "in");
                push_arguments(&mut xml, &method.output, "out");
                xml.push_str("    </method>\n");
            }
            let mut signals: Vec<_> = interface.signals.iter().collect();
            signals.sort_unstable_by_key(|(name, _)| name.as_str());
            for (name, signal) in signals {
                xml.push_str("    <signal name=\"");
                xml.push_str(name);
                xml.push_str("\">\n");
                push_signal_arguments(&mut xml, &signal.arguments);
                xml.push_str("    </signal>\n");
            }
            let mut properties: Vec<_> = interface.properties.iter().collect();
            properties.sort_unstable_by_key(|(name, _)| name.as_str());
            for (name, property) in properties {
                xml.push_str("    <property name=\"");
                xml.push_str(name);
                xml.push_str("\" type=\"");
                xml.push_str(property.signature());
                xml.push_str(if property.writable() {
                    "\" access=\"readwrite\">\n"
                } else {
                    "\" access=\"read\">\n"
                });
                xml.push_str(
                    "      <annotation name=\"org.freedesktop.DBus.Property.EmitsChangedSignal\" value=\"",
                );
                xml.push_str(property.change_mode().annotation());
                xml.push_str("\"/>\n    </property>\n");
            }
            xml.push_str("  </interface>\n");
        }
        xml.push_str("  <interface name=\"org.freedesktop.DBus.Introspectable\">\n");
        xml.push_str(
            "    <method name=\"Introspect\"><arg type=\"s\" direction=\"out\"/></method>\n",
        );
        xml.push_str("  </interface>\n");
        xml.push_str("  <interface name=\"org.freedesktop.DBus.Peer\">\n");
        xml.push_str("    <method name=\"Ping\"/>\n");
        if has_machine_id {
            xml.push_str(
                "    <method name=\"GetMachineId\"><arg type=\"s\" direction=\"out\"/></method>\n",
            );
        }
        xml.push_str("  </interface>\n");
        if has_properties(self) {
            xml.push_str(PROPERTIES_XML);
        }
        if self.object_manager {
            xml.push_str(OBJECT_MANAGER_XML);
        }
        for child in &self.children {
            xml.push_str("  <node name=\"");
            xml.push_str(child);
            xml.push_str("\"/>\n");
        }
        xml.push_str("</node>\n");
        self.introspection = xml;
    }
}

impl ObjectServer {
    /// Registers a typed signal contract for object introspection.
    ///
    /// Registration is metadata-only; emit the signal through
    /// [`crate::Connection::emit_signal`] or `emit_signal_to`.
    pub fn register_signal<B: Type>(
        &mut self,
        path: &str,
        interface: &str,
        name: &str,
    ) -> Result<()> {
        validate_path(path)?;
        validate_interface_name(interface, "interface name")?;
        validate_member_name(name, "signal name")?;
        if is_standard_interface(interface) {
            return Err(Error::ReservedInterface(interface.to_owned()));
        }

        let signals = &mut self.objects.entry(path.to_owned()).or_default();
        signals.registered = true;
        let signals = &mut signals
            .interfaces
            .entry(interface.to_owned())
            .or_default()
            .signals;
        if signals.contains_key(name) {
            return Err(Error::DuplicateSignal {
                path: path.to_owned(),
                interface: interface.to_owned(),
                signal: name.to_owned(),
            });
        }
        signals.insert(
            name.to_owned(),
            Signal {
                arguments: body_signatures::<B>(),
            },
        );
        self.refresh_introspection();
        Ok(())
    }
}

fn push_arguments(xml: &mut String, signatures: &[String], direction: &str) {
    for signature in signatures {
        xml.push_str("      <arg type=\"");
        xml.push_str(signature);
        xml.push_str("\" direction=\"");
        xml.push_str(direction);
        xml.push_str("\"/>\n");
    }
}

fn push_signal_arguments(xml: &mut String, signatures: &[String]) {
    for signature in signatures {
        xml.push_str("      <arg type=\"");
        xml.push_str(signature);
        xml.push_str("\"/>\n");
    }
}

pub(super) fn body_signatures<T: Type>() -> Vec<String> {
    match T::SIGNATURE {
        Signature::Unit => Vec::new(),
        Signature::Structure(fields) => fields.iter().map(Signature::to_string).collect(),
        signature => vec![signature.to_string()],
    }
}
