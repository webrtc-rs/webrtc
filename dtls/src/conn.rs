use std::collections::HashMap;

lazy_static! {
    pub static ref INVALID_KEYING_LABELS: HashMap<&'static str, bool> = {
        let mut map = HashMap::new();
        map.insert("client finished", true);
        map.insert("server finished", true);
        map.insert("master secret", true);
        map.insert("key expansion", true);
        map
    };
}
