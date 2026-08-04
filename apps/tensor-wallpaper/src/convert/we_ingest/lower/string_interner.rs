use super::*;

#[derive(Default)]
pub(super) struct StringInterner {
    ids: BTreeMap<String, SceneStringId>,
    pub(super) strings: Vec<String>,
}

impl StringInterner {
    pub(super) fn id(&mut self, value: &str) -> SceneStringId {
        if let Some(id) = self.ids.get(value) {
            return *id;
        }
        let id = SceneStringId(self.strings.len() as u32);
        self.strings.push(value.to_owned());
        self.ids.insert(value.to_owned(), id);
        id
    }

    pub(super) fn optional_id(&mut self, value: &str) -> SceneStringId {
        if value.is_empty() {
            SceneStringId::NONE
        } else {
            self.id(value)
        }
    }

    pub(super) fn finish(self) -> Vec<String> {
        self.strings
    }
}
