use bevy_ecs::{component::Component, entity::Entity, world::World};

use super::VisibleItemPath;
use crate::Entry;
use crate::ui::icon_roles::{FileIconRoleCacheKey, file_icon_role_cache_key_with_stamp};

#[derive(Component)]
struct VisibleItemIconRole {
    entry: Entry,
    role: FileIconRoleCacheKey,
}

pub(super) fn retained_icon_role_for_entry<'a>(
    world: &'a World,
    entity: Entity,
    entry: &Entry,
) -> Option<&'a FileIconRoleCacheKey> {
    let retained = world.get::<VisibleItemIconRole>(entity)?;
    Entry::ptr_eq(&retained.entry, entry).then_some(&retained.role)
}

pub(super) fn refresh_visible_icon_role(world: &mut World, entity: Entity, entry: &Entry) {
    if world
        .get::<VisibleItemIconRole>(entity)
        .is_some_and(|retained| Entry::ptr_eq(&retained.entry, entry))
    {
        return;
    }
    let Some(path) = world.get::<VisibleItemPath>(entity) else {
        return;
    };
    let role = file_icon_role_cache_key_with_stamp(
        path.0.as_ref(),
        entry.is_dir,
        entry.mime_type.clone(),
        entry.mime_magic_checked,
        entry.modified_secs,
    );
    world.entity_mut(entity).insert(VisibleItemIconRole {
        entry: entry.clone(),
        role,
    });
}
