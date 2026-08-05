use crate::{Error, Result};

use super::ObjectServer;

impl ObjectServer {
    pub(super) fn refresh_introspection(&mut self) {
        self.objects.retain(|_, object| object.registered);
        let registered_paths: Vec<_> = self.objects.keys().cloned().collect();
        for path in &registered_paths {
            for ancestor in ancestors(path) {
                self.objects.entry(ancestor).or_default();
            }
        }
        let paths: Vec<_> = self.objects.keys().cloned().collect();
        let has_machine_id = self.machine_id.is_some();
        for (path, object) in &mut self.objects {
            object.children = direct_children(path, &paths);
            object.rebuild_introspection(has_machine_id);
        }
    }
}

fn ancestors(path: &str) -> Vec<String> {
    if path == "/" {
        return Vec::new();
    }
    let mut ancestors = vec!["/".to_owned()];
    for (index, byte) in path.bytes().enumerate().skip(1) {
        if byte == b'/' {
            ancestors.push(path[..index].to_owned());
        }
    }
    ancestors
}

pub(super) fn is_descendant(root: &str, path: &str) -> bool {
    path != root
        && if root == "/" {
            path.starts_with('/')
        } else {
            path.strip_prefix(root)
                .is_some_and(|suffix| suffix.starts_with('/'))
        }
}

pub(super) fn validate_manager_relationship(
    server: &ObjectServer,
    manager_path: &str,
    object_path: &str,
) -> Result<()> {
    let manager = server
        .objects
        .get(manager_path)
        .filter(|object| object.registered)
        .ok_or_else(|| Error::InvalidName {
            kind: "object manager path",
            value: manager_path.to_owned(),
        })?;
    if !manager.object_manager || !is_descendant(manager_path, object_path) {
        return Err(Error::InvalidName {
            kind: "managed object path",
            value: object_path.to_owned(),
        });
    }
    if !server
        .objects
        .get(object_path)
        .is_some_and(|object| object.registered)
    {
        return Err(Error::InvalidName {
            kind: "registered object path",
            value: object_path.to_owned(),
        });
    }
    Ok(())
}

pub(super) fn validate_manager_path(server: &ObjectServer, manager_path: &str) -> Result<()> {
    let manager = server
        .objects
        .get(manager_path)
        .filter(|object| object.registered)
        .ok_or_else(|| Error::InvalidName {
            kind: "object manager path",
            value: manager_path.to_owned(),
        })?;
    if manager.object_manager {
        Ok(())
    } else {
        Err(Error::InvalidName {
            kind: "object manager path",
            value: manager_path.to_owned(),
        })
    }
}

pub(super) fn direct_children(parent: &str, paths: &[String]) -> Vec<String> {
    let prefix = if parent == "/" {
        "/".to_owned()
    } else {
        format!("{parent}/")
    };
    let mut children: Vec<_> = paths
        .iter()
        .filter_map(|path| {
            let suffix = path.strip_prefix(&prefix)?;
            (!suffix.is_empty() && !suffix.contains('/')).then(|| suffix.to_owned())
        })
        .collect();
    children.sort_unstable();
    children
}
